// client.rs — gRPC client for tridentd daemon

use serde::{Deserialize, Serialize};
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

// Include the proto-generated types from the build script
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/tridentd.rs"));
}

use proto::trident_daemon_client::TridentDaemonClient;

#[derive(Debug)]
pub struct DaemonClient {
    client: TridentDaemonClient<Channel>,
}

impl DaemonClient {
    pub async fn connect(addr: &str) -> Result<Self, tonic::transport::Error> {
        let endpoint = Endpoint::from_shared(addr.to_string())?;
        let client = TridentDaemonClient::connect(endpoint).await?;
        Ok(Self { client })
    }

    pub async fn ping(&mut self) -> Result<proto::PingResponse, tonic::Status> {
        let req = Request::new(proto::PingRequest {});
        let resp = self.client.ping(req).await?;
        Ok(resp.into_inner())
    }

    pub async fn launch_instance(
        &mut self,
        kernel_path: String,
        system_image: Option<String>,
        vendor_image: Option<String>,
        cmdline: String,
        vcpu_count: u32,
        memory_mib: u64,
        sriov_vf: Option<String>,
    ) -> Result<proto::InstanceInfo, tonic::Status> {
        let req = Request::new(proto::LaunchRequest {
            kernel_path,
            system_image: system_image.unwrap_or_default(),
            vendor_image: vendor_image.unwrap_or_default(),
            cmdline,
            vcpu_count,
            memory_mib,
            sriov_vf: sriov_vf.unwrap_or_default(),
            snapshot_id: String::new(),
        });
        let resp = self.client.launch_instance(req).await?;
        Ok(resp.into_inner())
    }

    pub async fn stop_instance(&mut self, instance_id: String) -> Result<bool, tonic::Status> {
        let req = Request::new(proto::StopRequest {
            instance_id,
            force: false,
        });
        let resp = self.client.stop_instance(req).await?;
        Ok(resp.into_inner().success)
    }

    pub async fn fork_instance(
        &mut self,
        instance_id: String,
        count: u32,
    ) -> Result<Vec<proto::InstanceInfo>, tonic::Status> {
        let req = Request::new(proto::ForkRequest { instance_id, count });
        let mut stream = self.client.fork(req).await?.into_inner();
        let mut children = Vec::new();
        while let Some(info) = stream.message().await? {
            children.push(info);
        }
        Ok(children)
    }
}

// Re-export proto types for commands.rs
pub use proto::*;
