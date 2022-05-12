extern crate core;

use crate::datastructures::Config;
use crate::socketlib::{SocketConn, VLCConn};
use anyhow::anyhow;
use clap::{arg, Command};
use log::{debug, error, info};
use soup::prelude::*;
use std::time::Duration;
use tokio::sync::oneshot::Receiver;

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
) -> anyhow::Result<RequestStatus> {
    if config.monitor().web_enabled() {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.monitor().interval() * 60))
            .build()
            .unwrap();
        let ret = client
            .post(config.monitor().backend())
            //.headers(header_map)
            .form(&[("usersuche", config.monitor().username()), ("username", "")])
            .send()
            .await
            .map_err(|e| anyhow!("Got error while send monitor request {:?}", e))?
            .text()
            .await
            .map_err(|e| anyhow!("Got error while get text from request: {:?}", e))?;
        let soup = Soup::new(&ret);
        let tds = soup
            .tag("tbody")
            .find()
            .map(|tbody| {
                tbody
                    .tag("tr")
                    .find()
                    .map(|tr| tr.tag("td").find_all().collect::<Vec<_>>())
            })
            .ok_or_else(|| anyhow!("Can't found any table."))?
            .ok_or_else(|| anyhow!("Can't found any result."))?;
        if tds.len() > 2 && tds[1].text().to_lowercase().eq("online") {
            return Ok(RequestStatus::Online);
        }
        conn.connect_server(config.server().address(), config.monitor().username())
            .await
            .map_err(|e| anyhow!("Connect to server failure: {:?}", e))?;
        conn.wait_timeout(Duration::from_secs(config.server().timeout()))
            .await
            .map_err(|_| anyhow!("Wait connect timeout"))??;
    } else {
        conn.connect_server(config.server().address(), config.monitor().username())
            .await
            .map_err(|e| anyhow!("Unable to connect server: {:?}", e))?;
        info!("Login to server (check)");

        conn.wait_timeout(Duration::from_secs(config.server().timeout()))
            .await
            .map_err(|_| anyhow!("Wait connect timeout"))??;
        debug!("Connected to server!");

        let my = conn
            .who_am_i()
            .await
            .map_err(|e| anyhow!("Got error while command whoami: {:?}", e))?;
        let clients = conn
            .query_clients()
            .await
            .map_err(|e| anyhow!("Got error while query clients: {:?}", e))?;
        let mut database_id = 0;
        for client in &clients {
            if client.client_id() == my.client_id() {
                database_id = client.client_database_id();
            }
        }
        if database_id == 0 {
            return Err(anyhow!("Can't get self database_id"));
        }

        if conn
            .check_self_duplicate()
            .await
            .map_err(|e| anyhow!("Got error while check self duplicate: {:?}", e))?
        {
            conn.disconnect()
                .await
                .map_err(|e| anyhow!("Got error while disconnect from server: {:?}", e))?;
            return Ok(RequestStatus::DuplicateClient);
        }
    }
    // Early check
    let monitor_id = config.monitor_id();
    if conn
        .query_clients()
        .await
        .map_err(|e| anyhow!("Got error while list clients: {:?}", e))?
        .into_iter()
        .any(|client| monitor_id.contains(&client.client_database_id()))
    {
        conn.disconnect().await?;
        return Ok(RequestStatus::TargetDetected);
    }

    conn.switch_channel_by_name(config.server().channel())
        .await
        .map_err(|e| anyhow!("Switch channel error: {:?}", e))?;
    tokio::time::sleep(Duration::from_micros(500)).await;
    if let Some(password) = config.server().password() {
        conn.set_current_channel_password(password)
            .await
            .map_err(|e| error!("Set password error, ignored: {:?}", e))
            .ok();
    }
    Ok(RequestStatus::NotOnline)
}

fn get_duration(config: &Config, times: u64, ret: RequestStatus) -> Duration {
    match ret {
        RequestStatus::Online => Duration::from_secs(config.monitor().interval() * 60),
        RequestStatus::TargetDetected | RequestStatus::DuplicateClient => Duration::from_secs(
            match times {
                1 => 5,
                2 => 30,
                3..=u64::MAX => 60,
                _ => {
                    unreachable!()
                }
            } * 60,
        ),
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

    let mut conn = SocketConn::connect(server, port)
        .await
        .map_err(|e| anyhow!("Connect teamspeak console error: {:?}", e))?;
    conn.login(configure.api_key()).await?;
    //conn.register_events().await??;

    let vlc_conn = VLCConn::connect("localhost", 4212, "1\n\r")
        .await
        .map_err(|e| anyhow!("Connect to VLC console error: {:?}", e))?;

    info!("Connected.");

    let mut times = 0;
    while let Err(e) = conn.who_am_i().await {
        if e.code() == 1794 {
            let ret = check_online_in_offline(&configure, &mut conn).await?;
            if ret == RequestStatus::NotOnline {
                break;
            }
            info!("Client duplicate or target confirm, wait more time to reconnect.");
            times += 1;
            if tokio::time::timeout(get_duration(&configure, times, ret), &mut receiver)
                .await
                .is_ok()
            {
                return Ok(());
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
        ret = running_loop(path, server ,port, receiver) =>  {
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
        .init();

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
