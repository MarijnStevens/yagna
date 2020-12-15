use actix::prelude::*;
use anyhow::{anyhow, bail, Error, Result};
use chrono::{Duration, Utc};
use futures::future::TryFutureExt;
use std::collections::HashMap;

use ya_utils_actix::actix_handler::ResultTypeGetter;
use ya_utils_actix::actix_signal::Subscribe;
use ya_utils_actix::forward_actix_handler;

use super::task_info::TaskInfo;
use super::task_state::{AgreementState, TasksStates};
use crate::execution::{ActivityCreated, ActivityDestroyed, TaskRunner};
use crate::market::provider_market::{AgreementApproved, ProviderMarket};
use crate::market::termination_reason::BreakReason;
use crate::payments::Payments;

// =========================================== //
// Messages modifying agreement state
// =========================================== //

/// These events can be sent to TaskManager:
/// - AgreementApproved
/// - ActivityCreated
/// - ActivityDestroyed
/// - BreakAgreement
/// - CloseAgreement

/// Event forces agreement termination, what includes killing ExeUnit.
/// Sending this event indicates, that agreement conditions were broken
/// somehow. Normally Requestor is responsible for agreement termination.
#[derive(Message, Clone)]
#[rtype(result = "Result<()>")]
pub struct BreakAgreement {
    pub agreement_id: String,
    pub reason: BreakReason,
}

/// Notifies TaskManager that Requestor close agreement.
#[derive(Message, Clone)]
#[rtype(result = "Result<()>")]
pub struct CloseAgreement {
    pub agreement_id: String,
    pub is_terminated: bool,
}

// =========================================== //
// Output events
// =========================================== //

/// Agreement was broken by us. All modules will get this message,
/// when TaskManager will get BreakAgreement event.
///
/// Note: This message can't fail. All modules that get this message,
/// must break Agreement and handle all potential errors internally.
/// TODO: How to return async ActorResponse from handler without Result?
#[derive(Message, Clone)]
#[rtype(result = "Result<()>")]
pub struct AgreementBroken {
    pub agreement_id: String,
    pub reason: BreakReason,
}

/// Agreement is finished by Requestor. This is proper way to close Agreement.
#[derive(Message, Clone)]
#[rtype(result = "Result<()>")]
pub struct AgreementClosed {
    pub agreement_id: String,
    pub send_terminate: bool,
}

// =========================================== //
// TaskManager messages not related to agreements
// =========================================== //

/// Initialize TaskManager.
#[derive(Message)]
#[rtype(result = "Result<()>")]
pub struct InitializeTaskManager;

// =========================================== //
// TaskManager internal messages
// =========================================== //

#[derive(Message)]
#[rtype(result = "Result<()>")]
struct ScheduleExpiration(TaskInfo);

#[derive(Message)]
#[rtype(result = "Result<()>")]
struct StartUpdateState {
    pub agreement_id: String,
    pub new_state: AgreementState,
}

#[derive(Message)]
#[rtype(result = "Result<()>")]
struct FinishUpdateState {
    pub agreement_id: String,
    pub new_state: AgreementState,
}

// =========================================== //
// TaskManager implementation
// =========================================== //

/// Task manager is responsible for managing tasks (agreements)
/// state. It controls whole flow of task execution from the point,
/// when it gets signed agreement from market, to the point of agreement payment.
pub struct TaskManager {
    market: Addr<ProviderMarket>,
    runner: Addr<TaskRunner>,
    payments: Addr<Payments>,

    tasks: TasksStates,
    tasks_props: HashMap<String, TaskInfo>,
}

impl TaskManager {
    pub fn new(
        market: Addr<ProviderMarket>,
        runner: Addr<TaskRunner>,
        payments: Addr<Payments>,
    ) -> Result<TaskManager> {
        Ok(TaskManager {
            market,
            runner,
            payments,
            tasks: TasksStates::new(),
            tasks_props: HashMap::new(),
        })
    }

    fn schedule_expiration(
        &mut self,
        msg: ScheduleExpiration,
        ctx: &mut Context<Self>,
    ) -> Result<()> {
        let agreement_id = msg.0.agreement_id;
        let expiration = msg.0.expiration;

        if Utc::now() > expiration {
            bail!(
                "Agreement expired before start. Expiration {:#?}",
                expiration
            );
        }

        // Schedule agreement termination after expiration time.
        let duration = (expiration - Utc::now()).to_std()?;
        let agr_id = agreement_id.clone();
        ctx.run_later(duration, move |myself, ctx| {
            if !myself.tasks.is_agreement_finalized(&agr_id) {
                ctx.address().do_send(BreakAgreement {
                    agreement_id: agr_id,
                    reason: BreakReason::Expired(expiration),
                });
            }
        });

        // Schedule agreement termination when there is no activity created within 90s.
        let s90 = Duration::seconds(90).to_std()?;
        ctx.run_later(s90.clone(), move |myself, ctx| {
            if myself.tasks.not_active(&agreement_id) {
                ctx.address().do_send(BreakAgreement {
                    agreement_id,
                    reason: BreakReason::NoActivity(s90),
                });
            }
        });
        Ok(())
    }

    fn start_update_agreement_state(
        &mut self,
        msg: StartUpdateState,
        _ctx: &mut Context<Self>,
    ) -> Result<()> {
        Ok(self
            .tasks
            .start_transition(&msg.agreement_id, msg.new_state)?)
    }

    fn finish_update_agreement_state(
        &mut self,
        msg: FinishUpdateState,
        _ctx: &mut Context<Self>,
    ) -> Result<()> {
        Ok(self
            .tasks
            .finish_transition(&msg.agreement_id, msg.new_state)?)
    }

    fn async_context(&self, ctx: &mut Context<Self>) -> TaskManagerAsyncContext {
        TaskManagerAsyncContext {
            runner: self.runner.clone(),
            payments: self.payments.clone(),
            market: self.market.clone(),
            myself: ctx.address().clone(),
        }
    }

    fn add_new_agreement(&mut self, msg: &AgreementApproved) -> anyhow::Result<TaskInfo> {
        let agreement_id = msg.agreement.agreement_id.clone();
        self.tasks.new_agreement(&agreement_id)?;

        let props = TaskInfo::from(&msg.agreement)
            .map_err(|e| anyhow!("Failed to create TaskInfo from Agreement. {}", e))?;

        self.tasks_props.insert(agreement_id.clone(), props.clone());

        self.tasks
            .start_transition(&agreement_id, AgreementState::Initialized)?;
        Ok(props)
    }
}

impl Actor for TaskManager {
    type Context = Context<Self>;
}

forward_actix_handler!(TaskManager, ScheduleExpiration, schedule_expiration);
forward_actix_handler!(TaskManager, StartUpdateState, start_update_agreement_state);
forward_actix_handler!(
    TaskManager,
    FinishUpdateState,
    finish_update_agreement_state
);

impl Handler<InitializeTaskManager> for TaskManager {
    type Result = ActorResponse<Self, (), Error>;

    fn handle(&mut self, _msg: InitializeTaskManager, ctx: &mut Context<Self>) -> Self::Result {
        let actx = self.async_context(ctx);

        let future = async move {
            // Listen to AgreementApproved event.
            let msg = Subscribe::<AgreementApproved>(actx.myself.clone().recipient());
            actx.market.send(msg).await??;

            // Listen to Agreement terminated event from market.
            let msg = Subscribe::<CloseAgreement>(actx.myself.clone().recipient());
            actx.market.send(msg).await??;

            // Get info about Activity creation and destruction.
            let msg = Subscribe::<ActivityCreated>(actx.myself.clone().recipient());
            actx.runner.send(msg).await??;

            let msg = Subscribe::<ActivityDestroyed>(actx.myself.clone().recipient());
            Ok(actx.runner.send(msg).await??)
        }
        .into_actor(self);

        ActorResponse::r#async(future)
    }
}

// =========================================== //
// Messages modifying agreement state
// =========================================== //

impl Handler<AgreementApproved> for TaskManager {
    type Result = ActorResponse<Self, (), Error>;

    fn handle(&mut self, msg: AgreementApproved, ctx: &mut Context<Self>) -> Self::Result {
        // Add new agreement with it's state.
        let task_info = match self.add_new_agreement(&msg) {
            Err(error) => {
                log::error!("{}", error);
                return ActorResponse::reply(Err(error));
            }
            Ok(task_info) => task_info,
        };

        if task_info.multi_activity {
            log::info!(
                "Agreement [{}] will be initialized in multi activity mode.",
                &task_info.agreement_id
            )
        }

        let actx = self.async_context(ctx);
        let agreement_id = task_info.agreement_id.clone();

        let future = async move {
            actx.myself.send(ScheduleExpiration(task_info)).await??;

            actx.runner.send(msg.clone()).await??;
            actx.payments.send(msg.clone()).await??;

            finish_transition(
                &actx.myself,
                &msg.agreement.agreement_id,
                AgreementState::Initialized,
            )
            .await
        }
        .into_actor(self)
        .map(
            move |result: Result<(), anyhow::Error>, _, context: &mut Context<Self>| {
                if let Err(error) = result {
                    // If initialization failed, the only thing, we can do is breaking agreement.
                    let msg = BreakAgreement {
                        agreement_id: agreement_id.clone(),
                        reason: BreakReason::InitializationError {
                            error: format!("{}", error),
                        },
                    };
                    context.address().do_send(msg);
                }
            },
        );

        ActorResponse::r#async(future.map(|_, _, _| Ok(())))
    }
}

impl Handler<ActivityCreated> for TaskManager {
    type Result = ActorResponse<Self, (), Error>;

    fn handle(&mut self, msg: ActivityCreated, _ctx: &mut Context<Self>) -> Self::Result {
        // TODO: Consider different flow: TaskRunner sends us request for creating activity
        //       and TaskManager is responsible for sending CreateActivity to runner.
        let listener = self.tasks.changes_listener(&msg.agreement_id);
        let future = async move {
            // ActivityCreated event can come, before Task initialization will be finished.
            // In this case we must wait, because otherwise transition to Computing will fail.
            let mut state = listener?;
            state.transition_finished().await
        }
        .into_actor(self)
        .map(move |result, myself, _| {
            // Return, if waiting for transition failed.
            // This indicates, that State was already dropped.
            result
                .map_err(|e| anyhow!("[ActivityCreated] Can't change state to Computing. {}", e))?;
            let agreement_id = msg.agreement_id.clone();

            myself
                .tasks
                .start_transition(&agreement_id, AgreementState::Computing)?;

            // Forward information to Payments for cost computing.
            myself.payments.do_send(msg);
            myself
                .tasks
                .finish_transition(&agreement_id, AgreementState::Computing)?;
            anyhow::Result::<()>::Ok(())
        })
        .map(|result, _, _| match result {
            Err(e) => Ok(log::error!("[ActivityCreated] {}", e)),
            Ok(()) => Ok(()),
        });

        ActorResponse::r#async(future)
    }
}

impl Handler<ActivityDestroyed> for TaskManager {
    type Result = ActorResponse<Self, (), Error>;

    fn handle(&mut self, msg: ActivityDestroyed, ctx: &mut Context<Self>) -> Self::Result {
        let agreement_id = msg.agreement_id.clone();
        let actx = self.async_context(ctx);

        // TODO: We should somehow reject ActivityDestroyed messages, when Agreement doesn't exist.
        let closing_allowed = self
            .tasks
            .allowed_transition(&agreement_id, &AgreementState::Closed)
            .is_ok();

        let close_after_1st_activity = self
            .tasks_props
            .get(&agreement_id)
            .map(|info| !info.multi_activity)
            .unwrap_or(false);

        // Provider is responsible for closing Agreement, if multi_activity flag wasn't
        // set in Agreement. Otherwise Requestor should terminate.
        let need_close = closing_allowed && close_after_1st_activity;

        let future = async move {
            // Forward information to Payments to send last DebitNote in activity.
            // TODO: What can we do in case of fail? Payments are expected to retry
            //       after they will succeed.
            actx.payments.send(msg).await??;

            if need_close {
                log::info!(
                    "First activity for agreement [{}] destroyed. Closing since \
                    task was in single activity mode.",
                    &agreement_id
                );

                actx.myself.do_send(CloseAgreement {
                    agreement_id: agreement_id.to_string(),
                    is_terminated: false,
                });
            }
            Ok(())
        };
        ActorResponse::r#async(future.into_actor(self))
    }
}

impl Handler<BreakAgreement> for TaskManager {
    type Result = ActorResponse<Self, (), Error>;

    fn handle(&mut self, msg: BreakAgreement, ctx: &mut Context<Self>) -> Self::Result {
        let actx = self.async_context(ctx);

        let future = async move {
            let new_state = AgreementState::Broken {
                reason: msg.reason.clone(),
            };

            log::warn!(
                "Breaking agreement [{}], reason: {}.",
                msg.agreement_id,
                msg.reason
            );

            start_transition(&actx.myself, &msg.agreement_id, new_state.clone()).await?;

            let result = async move {
                let msg = AgreementBroken::from(msg.clone());
                actx.runner.send(msg.clone()).await??;
                // Notify market, but we don't care about result.
                // TODO: Breaking agreement shouldn't fail at anytime. But in current code we can
                //       return early, before we notify market.
                actx.market.do_send(msg.clone());

                actx.payments.send(msg.clone()).await??;

                finish_transition(&actx.myself, &msg.agreement_id, new_state).await?;

                log::info!("Agreement [{}] cleanup finished.", msg.agreement_id);
                Ok(())
            }
            .await;

            result
        }
        .map_err(move |error: Error| log::error!("Can't break agreement. Error: {}", error));

        ActorResponse::r#async(future.into_actor(self).map(|_, _, _| Ok(())))
    }
}

impl Handler<CloseAgreement> for TaskManager {
    type Result = ActorResponse<Self, (), Error>;

    fn handle(&mut self, msg: CloseAgreement, ctx: &mut Context<Self>) -> Self::Result {
        let actx = self.async_context(ctx);

        // TODO: Probably if closing agreement fails, we should break agreement.
        //       Here lacks this error handling, we just log message.
        let future = async move {
            start_transition(&actx.myself, &msg.agreement_id, AgreementState::Closed).await?;

            let msg = AgreementClosed {
                agreement_id: msg.agreement_id.clone(),
                send_terminate: !msg.is_terminated,
            };

            actx.runner.do_send(msg.clone());
            actx.market.do_send(msg.clone());
            actx.payments.send(msg.clone()).await??;

            finish_transition(&actx.myself, &msg.agreement_id, AgreementState::Closed).await?;

            log::info!("Agreement [{}] cleanup finished.", msg.agreement_id);
            Ok(())
        }
        .map_err(move |error: Error| log::error!("Can't close agreement. Error: {}", error));

        ActorResponse::r#async(future.into_actor(self).map(|_, _, _| Ok(())))
    }
}

// =========================================== //
// Helper implementations - no need to read below
// =========================================== //

async fn start_transition(
    myself: &Addr<TaskManager>,
    agreement_id: &str,
    new_state: AgreementState,
) -> Result<()> {
    let msg = StartUpdateState {
        agreement_id: agreement_id.to_string(),
        new_state,
    };
    Ok(myself.clone().send(msg).await??)
}

async fn finish_transition(
    myself: &Addr<TaskManager>,
    agreement_id: &str,
    new_state: AgreementState,
) -> Result<()> {
    let msg = FinishUpdateState {
        agreement_id: agreement_id.to_string(),
        new_state,
    };
    Ok(myself.clone().send(msg).await??)
}

/// Helper struct storing TaskManager sub-actors addresses to use in async functions.
struct TaskManagerAsyncContext {
    pub runner: Addr<TaskRunner>,
    pub payments: Addr<Payments>,
    pub market: Addr<ProviderMarket>,
    pub myself: Addr<TaskManager>,
}

impl From<BreakAgreement> for AgreementBroken {
    fn from(msg: BreakAgreement) -> Self {
        AgreementBroken {
            agreement_id: msg.agreement_id,
            reason: msg.reason,
        }
    }
}