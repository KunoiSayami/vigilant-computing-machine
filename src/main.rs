use crate::datastructures::Config;
use crate::socketlib::{SocketConn, VLCConn};
use anyhow::anyhow;
use clap::{arg, Command};
use log::{debug, error, info};
use soup::prelude::*;
use std::future;
use std::time::Duration;
use tokio::sync::oneshot::Receiver;
use tsclientlib::{Connection, Identity};

#[allow(dead_code)]
mod datastructures;
mod socketlib;

async fn real_staff(
    mut conn: SocketConn,
    mut vlc_con: VLCConn,
    mut recv: Receiver<bool>,
    monitor: Vec<i64>,
    need_exit: bool,
) -> anyhow::Result<()> {
    let database_id = conn
        .query_database_id()
        .await
        .map_err(|e| anyhow!("Got query database id error: {:?}", e))?;
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
            .iter()
            .any(|client| monitor.contains(&client.client_database_id()));

        let status = vlc_con
            .get_status()
            .await
            .map_err(|e| anyhow!("Got error while fetch VLC status: {:?}", e))?;

        if cared_user && need_exit {
            info!("Need exit set to true, exit client.");
            conn.disconnect().await?;
            break;
        }

        if clients
            .iter()
            .filter(|&n| n.client_database_id() == database_id)
            .count()
            > 1
        {
            info!("Find duplicate session in server, disconnect.");
            conn.disconnect()
                .await
                .map_err(|e| anyhow!("Got error while disconnect from server: {:?}", e))?;
        }

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

#[derive(PartialEq)]
enum RequestStatus {
    Online,
    /// DUPLICATE CLIENT OR TARGET DETECTED
    TargetDetected,
    DuplicateClient,
    NotOnline,
}

async fn check_online_in_offline(
    config: &Config,
    conn: &mut SocketConn,
    nickname: &str,
) -> anyhow::Result<RequestStatus> {
    todo!()
}

fn get_duration(config: &Config, times: u64, ret: RequestStatus) -> u64 {
    match ret {
        RequestStatus::Online => config.monitor().interval() * 60,
        RequestStatus::TargetDetected | RequestStatus::DuplicateClient => {
            (match times {
                1 => 5,
                2 => 30,
                3..=u64::MAX => 60,
                _ => unreachable!(),
            }) * 60
        }
        RequestStatus::NotOnline => unreachable!(),
    }
}

async fn running_loop(
    path: &str,
    server: &str,
    port: u16,
    mut receiver: Receiver<bool>,
) -> anyhow::Result<()> {
    let configure = Config::try_from(path.as_ref())
        .map_err(|e| anyhow!("Read configure file error: {:?}", e))?;

    let nickname = {
        let default = configure
            .server()
            .username()
            .clone()
            .unwrap_or_else(|| configure.monitor().username().to_string());
        if configure.show_bot_tag() {
            format!("{}(bot)", default)
        } else {
            default
        }
    };
    info!("Working.");
    let mut conn = Connection::build(&configure.server().address())
        .name(nickname)
        .version(tsclientlib::Version::Linux_3_5_6)
        .channel(configure.server().channel())
        .identity(Identity::new_from_str(configure.identity()).unwrap())
        .connect()
        .map_err(|e| anyhow!("Connect failure: {:?}", e))?;

    conn.events()
        // We are connected when we receive the first BookEvents
        .try_filter(|e| future::ready(matches!(e, StreamItem::BookEvents(_))))
        .next()
        .await
        .unwrap();

    let mut times = 0;
    while let Err(e) = conn.who_am_i().await {
        if e.code() == 1794 {
            let ret = check_online_in_offline(&configure, &mut conn, &nickname).await?;
            if ret == RequestStatus::NotOnline {
                break;
            }
            if ret != RequestStatus::Online {
                info!("Client duplicate or target confirm, wait more time to reconnect.");
            }
            times += 1;
            let duration = get_duration(&configure, times, ret);
            for _ in 0..duration / 30 {
                conn.who_am_i().await.ok();
                if tokio::time::timeout(Duration::from_secs(30), &mut receiver)
                    .await
                    .is_ok()
                {
                    return Ok(());
                }
            }
        } else {
            return Err(anyhow::Error::from(e));
        }
    }
    real_staff(
        conn,
        vlc_conn,
        receiver,
        configure.monitor_id(),
        configure.need_disconnect(),
    )
    .await?;
    Ok(())
}

async fn staff(path: &str, server: &str, port: u16) -> anyhow::Result<()> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tokio::select! {
        _ = async move {
            tokio::signal::ctrl_c().await.unwrap();
            sender.send(true).unwrap();
            info!("Recv SIGINT signal, send exit signal");
            tokio::signal::ctrl_c().await.unwrap();
            info!("Recv SIGINT again, force exit.");
            std::process::exit(137);
        } => {}
        ret = running_loop(path, server, port, receiver) =>  {
           ret?
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

    env_logger::Builder::from_default_env()
        .filter_module("html5ever", log::LevelFilter::Warn)
        .filter_module("reqwest", log::LevelFilter::Warn)
        .init();

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(staff(
            matches.get_one("CONFIGURE").unwrap(),
            "localhost",
            25639,
        ))?;

    Ok(())
}
