use crate::datastructures::Config;
use crate::socketlib::{SocketConn, VLCConn};
use anyhow::anyhow;
use clap::{arg, Command};
use log::info;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot::Receiver;
use tokio::sync::Mutex;

#[allow(dead_code)]
mod datastructures;
mod socketlib;

async fn real_staff(
    mut conn: SocketConn,
    mut vlc_con: VLCConn,
    mut recv: Receiver<bool>,
    monitor: Vec<i64>,
) -> anyhow::Result<()> {
    loop {
        if recv.try_recv().is_ok() {
            info!("Exit!");
            return Ok(());
        }

        let clients = conn
            .query_clients()
            .await
            .map_err(|e| anyhow!("Got error while query clients: {:?}", e))?;

        let cared_user = clients
            .into_iter()
            .any(|client| monitor.contains(&client.client_database_id()));

        let status = vlc_con
            .get_status()
            .await
            .map_err(|e| anyhow!("Got error while fetch VLC status: {:?}", e))?;

        if status == cared_user {
            if status {
                vlc_con.pause().await?;
            } else {
                vlc_con.play().await?;
            }
            info!("Toggle to {}", if status { "pause" } else { "play" });
        }

        if tokio::time::timeout(Duration::from_millis(5), &mut recv)
            .await
            .is_ok()
        {
            break;
        }
    }
    Ok(())
}

async fn staff(path: &str, server: &str, port: u16) -> anyhow::Result<()> {
    let configure = Config::try_from(path.as_ref())
        .map_err(|e| anyhow!("Read configure file error: {:?}", e))?;

    let mut conn = SocketConn::connect(server, port)
        .await
        .map_err(|e| anyhow!("Connect teamspeak console error: {:?}", e))?;
    conn.login(configure.api_key()).await?;
    //conn.register_events().await??;

    let vlc_conn = VLCConn::connect("localhost", 4212, "1\n\r")
        .await
        .map_err(|e| anyhow!("Connect to VLC console error: {:?}", e))?;

    info!("Connected.");

    let (sender, receiver) = tokio::sync::oneshot::channel();
    let keepalive_signal = Arc::new(Mutex::new(false));
    tokio::select! {
        _ = async move {
            tokio::signal::ctrl_c().await.unwrap();
            sender.send(true).unwrap();
            info!("Recv SIGINT signal, send exit signal");
            tokio::signal::ctrl_c().await.unwrap();
            info!("Recv SIGINT again, force exit.");
            std::process::exit(137);
        } => {}
        _ = async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let mut i = keepalive_signal.lock().await;
                *i = true;
            }
        } => {}
        ret = real_staff(conn, vlc_conn, receiver, vec![configure.monitor_id()]) => {
           ret?;
        }
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .args(&[
            arg!([CONFIGURE] "Configure file"),
            arg!(--server "Specify server"),
            arg!(--port "Specify port"),
        ])
        .get_matches();

    env_logger::Builder::from_default_env().init();

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(staff(
            matches.value_of("CONFIGURE").unwrap(),
            "localhost",
            25639,
        ))?;

    Ok(())
}
