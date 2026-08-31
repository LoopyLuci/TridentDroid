use tridentd_lib::{platform, server, vmm};

use anyhow::{bail, Result};
use tracing::info;
use tracing_subscriber::EnvFilter;

struct Args {
    mode: Mode,
}

enum Mode {
    Serve,
    Single(vmm::VmConfig),
}

fn parse_args() -> Result<Args> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut it = raw.iter().peekable();

    let mut kernel_path = String::new();
    let mut initrd_path: Option<String> = None;
    let mut cmdline =
        "console=ttyS0 earlyprintk=serial androidboot.hardware=trident".to_string();
    let mut vcpu_count: u8 = 4;
    let mut memory_mib: u64 = 4096;
    let mut sriov_vf: Option<String> = None;
    let mut serve = false;
    let mut single = false;
    let mut system_image: Option<String> = None;
    let mut vendor_image: Option<String> = None;
    let mut console_sock: Option<String> = None;

    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--serve"    => serve = true,
            "--vm-single" => single = true,
            "--kernel"   => kernel_path = it.next().ok_or_else(|| anyhow::anyhow!("--kernel needs a path"))?.clone(),
            "--initrd"   => initrd_path = Some(it.next().ok_or_else(|| anyhow::anyhow!("--initrd needs a path"))?.clone()),
            "--args"     => cmdline = it.next().ok_or_else(|| anyhow::anyhow!("--args needs a string"))?.clone(),
            "--vcpus"    => vcpu_count = it.next().ok_or_else(|| anyhow::anyhow!("--vcpus needs N"))?.parse()?,
            "--mem"      => memory_mib = it.next().ok_or_else(|| anyhow::anyhow!("--mem needs MiB"))?.parse()?,
            "--sriov-vf" => sriov_vf = Some(it.next().ok_or_else(|| anyhow::anyhow!("--sriov-vf needs PCI addr"))?.clone()),
            "--system"   => system_image = Some(it.next().ok_or_else(|| anyhow::anyhow!("--system needs a path"))?.clone()),
            "--vendor"   => vendor_image = Some(it.next().ok_or_else(|| anyhow::anyhow!("--vendor needs a path"))?.clone()),
            "--console-sock" => console_sock = Some(it.next().ok_or_else(|| anyhow::anyhow!("--console-sock needs a path"))?.clone()),
            other        => bail!("Unknown flag: {}", other),
        }
    }

    if serve {
        return Ok(Args { mode: Mode::Serve });
    }
    if !single || kernel_path.is_empty() {
        bail!("Usage: tridentd --serve  |  tridentd --vm-single --kernel <bzImage> [options]");
    }

    Ok(Args {
        mode: Mode::Single(vmm::VmConfig {
            vcpu_count,
            memory_mib,
            kernel_path,
            initrd_path,
            cmdline,
            sriov_vf,
            system_image,
            vendor_image,
            console_sock,
        }),
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("tridentd=info".parse()?)
                .add_directive("trident_hal=info".parse()?),
        )
        .init();

    info!(
        "TridentDroid v{} — platform: {}",
        env!("CARGO_PKG_VERSION"),
        if cfg!(windows) { "Windows/WHP" } else { "Linux/KVM" }
    );

    let hyp = std::sync::Arc::new(platform::open_hypervisor()?);
    let args = parse_args()?;

    match args.mode {
        Mode::Serve => {
            info!("Starting gRPC/mTLS server on [::1]:9550");
            server::serve().await?;
        }
        Mode::Single(config) => {
            info!("Launching VM: {:?}", config);
            let vm = vmm::Vm::create(&hyp, config)?;
            vm.run().await?;
        }
    }

    Ok(())
}
