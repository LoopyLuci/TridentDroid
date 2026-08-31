// client.rs — gRPC client for tridentd daemon

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub host: String,
    pub port: u16,
    pub use_tls: bool,
    pub ca_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
    pub client_key_path: Option<String>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9550,
            use_tls: false,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
        }
    }
}

impl DaemonClient {
    pub async fn connect_with_config(config: &ConnectionConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let addr = format!("{}:{}", config.host, config.port);
        let scheme = if config.use_tls { "https" } else { "http" };
        let endpoint = Endpoint::from_shared(format!("{}://{}", scheme, addr))?;

        let channel = if config.use_tls {
            // mTLS configuration
            let ca_pem = fs::read(config.ca_cert_path.as_ref().ok_or("CA cert path required")?)?;
            let ca_cert = Certificate::from_pem(ca_pem);

            let tls = if let (Some(cert_path), Some(key_path)) = (&config.client_cert_path, &config.client_key_path) {
                let cert_pem = fs::read(cert_path)?;
                let key_pem = fs::read(key_path)?;
                let identity = Identity::from_pem(cert_pem, key_pem);
                ClientTlsConfig::new()
                    .ca_certificate(ca_cert)
                    .identity(identity)
            } else {
                ClientTlsConfig::new().ca_certificate(ca_cert)
            };

            endpoint.tls_config(tls)?.connect().await?
        } else {
            endpoint.connect().await?
        };

        let client = TridentDaemonClient::new(channel);
        Ok(Self { client })
    }

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

    pub async fn list_instances(&mut self) -> Result<Vec<proto::InstanceInfo>, tonic::Status> {
        let req = Request::new(proto::ListInstancesRequest {});
        let resp = self.client.list_instances(req).await?;
        Ok(resp.into_inner().instances)
    }

    pub async fn get_instance(&mut self, instance_id: String) -> Result<Option<proto::InstanceInfo>, tonic::Status> {
        let req = Request::new(proto::GetInstanceRequest { instance_id });
        match self.client.get_instance(req).await {
            Ok(resp) => Ok(Some(resp.into_inner())),
            Err(e) if e.code() == tonic::Code::NotFound => Ok(None),
            Err(e) => Err(e),
        }
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

    pub async fn adb_shell(
        &mut self,
        instance_id: String,
        mut input_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    ) -> Result<tokio::sync::mpsc::Receiver<Vec<u8>>, tonic::Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let (resp_tx, resp_rx) = tokio::sync::mpsc::channel(64);

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let mut resp_stream = self.client.adb_shell(Request::new(stream)).await?.into_inner();

        // Forward commands to daemon
        tokio::spawn(async move {
            while let Some(data) = input_rx.recv().await {
                let req = proto::AdbShellRequest {
                    instance_id: instance_id.clone(),
                    stdin_data: data,
                    eof: false,
                };
                if tx.send(req).await.is_err() {
                    break;
                }
            }
        });

        // Receive responses from daemon
        tokio::spawn(async move {
            while let Some(resp) = resp_stream.message().await.ok().flatten() {
                if resp_tx.send(resp.stdout_data).await.is_err() {
                    break;
                }
            }
        });

        Ok(resp_rx)
    }

    pub async fn stream_display(
        &mut self,
        instance_id: String,
    ) -> Result<tokio::sync::mpsc::Receiver<proto::DisplayFrame>, tonic::Status> {
        let req = Request::new(proto::DisplayStreamRequest {
            instance_id,
            codec: proto::VideoCodec::H264 as i32,
            target_fps: 60,
            bitrate_kbps: 5000,
        });

        let mut stream = self.client.stream_display(req).await?.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        tokio::spawn(async move {
            while let Some(frame) = stream.message().await.ok().flatten() {
                if tx.send(frame).await.is_err() {
                    break;
                }
            }
        });

        Ok(rx)
    }
}

// Re-export proto types for commands.rs
pub use proto::*;
