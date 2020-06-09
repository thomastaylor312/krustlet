//! Container statuses
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::{
    ContainerState, ContainerStateRunning, ContainerStateTerminated, ContainerStateWaiting,
    ContainerStatus as KubeContainerStatus, Pod as KubePod, PodCondition as KubePodCondition,
    PodStatus as KubePodStatus,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::client::Client;
use kube::{api::PatchParams, Api};
use log::error;
use tokio::sync::mpsc::{self, Receiver, Sender};

use std::collections::HashMap;
use std::fmt;

use crate::Pod;

const BUFFER_SIZE_DEFAULT: usize = 100;

/// An enum representing all available options for the status field of a [`PodCondition`] in the
/// Kubernetes API https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#pod-conditions
#[derive(Debug, serde::Serialize)]
pub enum PodConditionStatus {
    /// Equivalent to boolean true
    True,
    /// Equivalent to boolean false
    False,
    /// To be used when the current status of the condition cannot be ascertained
    Unknown,
}

impl fmt::Display for PodConditionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PodConditionStatus::True => write!(f, "True"),
            PodConditionStatus::False => write!(f, "False"),
            PodConditionStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

/// All allowed pod conditions with a helpful
#[derive(Debug)]
pub enum PodCondition {
    /// All init containers have completed successfully
    Initalized(PodConditionStatus),
    /// Pod is able to serve requests and should be added to the load balancing pools of matching
    /// services
    Ready(PodConditionStatus),
    /// All containers in the pod are ready
    ContainersReady(PodConditionStatus),
}

impl PodCondition {
    fn status(&self) -> &PodConditionStatus {
        match self {
            PodCondition::Initalized(s)
            | PodCondition::Ready(s)
            | PodCondition::ContainersReady(s) => s,
        }
    }
}

impl fmt::Display for PodCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PodCondition::Initalized(_) => write!(f, "Initalized"),
            PodCondition::Ready(_) => write!(f, "Ready"),
            PodCondition::ContainersReady(_) => write!(f, "ContainersReady"),
        }
    }
}

// From and Into are not symmetrical here, so we need to just implement Into
impl Into<KubePodCondition> for PodCondition {
    fn into(self) -> KubePodCondition {
        KubePodCondition {
            last_transition_time: Some(Time(chrono::Utc::now())),
            status: self.status().to_string(),
            type_: self.to_string(),
            ..Default::default()
        }
    }
}

/// Describe the status of a workload.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// Allows a provider to set a custom message, otherwise, kubelet will infer
    /// a message from the container statuses
    pub message: Option<String>,
    /// The statuses of containers keyed off their names
    pub container_statuses: HashMap<String, ContainerStatus>,
}

/// ContainerStatus is a simplified version of the Kubernetes container status for use in providers.
/// It allows for simple creation of the current status of a "container" (a running wasm process)
/// without worrying about a bunch of Options. Use the
/// [to_kubernetes](ContainerStatus::to_kubernetes) method for converting it to a Kubernetes API
/// container status
#[derive(Clone, Debug)]
pub enum ContainerStatus {
    /// The container is in a waiting state
    Waiting {
        /// The timestamp of when this status was reported
        timestamp: DateTime<Utc>,
        /// A human readable string describing the why it is in a waiting status
        message: String,
    },
    /// The container is running
    Running {
        /// The timestamp of when this status was reported
        timestamp: DateTime<Utc>,
    },
    /// The container is terminated
    Terminated {
        /// The timestamp of when this status was reported
        timestamp: DateTime<Utc>,
        /// A human readable string describing the why it is in a terminating status
        message: String,
        /// Should be set to true if the process exited with an error
        failed: bool,
    },
}

impl ContainerStatus {
    /// Convert the container status to a Kubernetes API compatible type
    pub fn to_kubernetes(&self, container_name: String) -> KubeContainerStatus {
        let mut state = ContainerState::default();
        match self {
            Self::Waiting { message, .. } => {
                state.waiting.replace(ContainerStateWaiting {
                    message: Some(message.clone()),
                    ..Default::default()
                });
            }
            Self::Running { timestamp } => {
                state.running.replace(ContainerStateRunning {
                    started_at: Some(Time(*timestamp)),
                });
            }
            Self::Terminated {
                timestamp,
                message,
                failed,
            } => {
                state.terminated.replace(ContainerStateTerminated {
                    finished_at: Some(Time(*timestamp)),
                    message: Some(message.clone()),
                    exit_code: *failed as i32,
                    ..Default::default()
                });
            }
        };
        let ready = state.running.is_some();
        KubeContainerStatus {
            state: Some(state),
            name: container_name,
            // Right now we don't have a way to probe, so just set to ready if
            // in a running state
            ready,
            // This is always true if startupProbe is not defined. When we
            // handle probes, this should be updated accordingly
            started: Some(true),
            // The rest of the items in status (see docs here:
            // https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.17/#containerstatus-v1-core)
            // either don't matter for us or we have not implemented the
            // functionality yet
            ..Default::default()
        }
    }
}

/// Describe the lifecycle phase of a workload.
///
/// This is specified by Kubernetes itself.
#[derive(Clone, Debug, serde::Serialize)]
pub enum Phase {
    /// The workload is currently executing.
    Running,
    /// The workload has exited with an error.
    Failed,
    /// The workload has exited without error.
    Succeeded,
    /// The lifecycle phase of the workload cannot be determined.
    Unknown,
}

impl Default for Phase {
    fn default() -> Self {
        Self::Unknown
    }
}

/// A convenience type around a [`Sender`] of a Pod and generic tuple
pub type StatusSender<T> = Sender<(Pod, T)>;
type StatusReceiver<T> = Receiver<(Pod, T)>;

/// A generic pod status updater that will enqueue incoming events of the given type and update the
/// pod status. If you want to provide your own status func implementation, you can use the
/// [new](PodStatusHandler::new) function. Otherwise there are two shorthands for the common status
/// updates: [new_container_status](PodStatusHandler::new_container_status) and
/// [new_pod_condition](PodStatusHandler::new_pod_condition)
pub struct PodStatusHandler<T> {
    stopper: Sender<()>,
    sender: StatusSender<T>,
}

impl<T: Send + Sync + 'static> PodStatusHandler<T> {
    /// Creates a new [`PodStatusHandler`] using the given kubernetes client and an optional buffer
    /// size for events. The default if no buffer size is specified is 100. This will start an
    /// underlying task that will listen for incoming events on the sender `channel`. The
    /// status_func must be a function that takes a Pod + any arbitrary type and turns it into a
    /// Kubernetes Pod Status object. For the most common usages see
    /// [new_container_status](PodStatusHandler::new_container_status) and
    /// [new_pod_condition](PodStatusHandler::new_pod_condition)
    pub fn new<F>(
        client: Client,
        buffer_size: Option<usize>,
        status_func: F,
    ) -> anyhow::Result<PodStatusHandler<T>>
    where
        F: Fn(Pod, T) -> KubePodStatus + Send + Sync + 'static,
    {
        let (sender, receiver): (StatusSender<T>, StatusReceiver<T>) =
            mpsc::channel(buffer_size.unwrap_or(BUFFER_SIZE_DEFAULT));
        // Note: we can't use a oneshot here because it gets moved into the select. So just use a
        // bounded channel of 1 so we can just do a mutable borrow
        let (stopper, stop) = mpsc::channel(1);
        tokio::spawn(run_status_loop(receiver, stop, client, status_func));
        Ok(PodStatusHandler::<T> { stopper, sender })
    }

    /// Creates a new `PodStatusHandler<ContainerStatus>` using the given kubernetes client and an
    /// optional buffer size for events. The default if no buffer size is specified is 100. This
    /// will start an underlying task that will listen for incoming events on the sender `channel`.
    pub fn new_container_status(
        client: Client,
        buffer_size: Option<usize>,
    ) -> anyhow::Result<PodStatusHandler<ContainerStatus>> {
        PodStatusHandler::new(client, buffer_size, container_status)
    }

    /// Creates a new `PodStatusHandler<PodCondition>` using the given kubernetes client and an
    /// optional buffer size for events. The default if no buffer size is specified is 100. This
    /// will start an underlying task that will listen for incoming events on the sender `channel`.
    pub fn new_pod_condition(
        client: Client,
        buffer_size: Option<usize>,
    ) -> anyhow::Result<PodStatusHandler<PodCondition>> {
        PodStatusHandler::new(client, buffer_size, pod_condition)
    }

    /// Returns a sender that can be used for sending events to the handler
    pub fn channel(&self) -> StatusSender<T> {
        self.sender.clone()
    }
}

async fn run_status_loop<T, F>(
    mut receiver: StatusReceiver<T>,
    mut stop: Receiver<()>,
    client: Client,
    status_func: F,
) where
    F: Fn(Pod, T) -> KubePodStatus + Send + Sync,
{
    loop {
        tokio::select! {
            _ = stop.recv() => return,
            res = receiver.recv() => {
                if let Some(data) = res {
                    let namespace = data.0.namespace().to_owned();
                    let name = data.0.name().to_owned();
                    let pod_status = status_func(data.0, data.1);
                    if let Err(e) = update_pod_status(client.clone(), &namespace, &name, &pod_status).await {
                        error!("unable to update status of pod {} in namespace {}: {}", name, namespace, e);
                    }
                } else {
                    error!("No remaining senders for status, there is likely a problem")
                }
            }
        }
    }
}

fn container_status(pod: Pod, cs: ContainerStatus) -> KubePodStatus {
    todo!("yep");
}

fn pod_condition(pod: Pod, condition: PodCondition) -> KubePodStatus {
    todo!("yep");
}

/// A helper for updating pod status. The given data should be a pod status object and be
/// serializable by serde
pub async fn update_pod_status<T: serde::Serialize>(
    client: kube::Client,
    ns: &str,
    pod_name: &str,
    data: &T,
) -> anyhow::Result<()> {
    let data = serde_json::to_vec(data)?;
    let pod_client: Api<KubePod> = Api::namespaced(client, ns);
    if let Err(e) = pod_client
        .patch_status(pod_name, &PatchParams::default(), data)
        .await
    {
        return Err(e.into());
    }
    Ok(())
}
