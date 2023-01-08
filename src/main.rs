use crate::datastructures::ClientVariable;
use crate::socketlib::SocketConn;
use anyhow::anyhow;
use clap::{arg, Command};
use log::info;
use rand::distributions::{Distribution, Uniform};
use std::time::Duration;
use tokio::sync::oneshot::Receiver;

#[allow(dead_code)]
mod datastructures;
mod socketlib;

async fn real_staff(
    mut conn: SocketConn,
    mut recv: Receiver<bool>,
    variable: ClientVariable,
) -> anyhow::Result<()> {
    let database_id = conn
        .query_database_id()
        .await
        .map_err(|e| anyhow!("Got query database id error: {:?}", e))?;

    let mut rng = rand::thread_rng();
    let die = Uniform::from(50..70);
    loop {
        if recv.try_recv().is_ok() {
            info!("Exit!");
            return Ok(());
        }

        conn.update_client_description(variable.clone().into_edit(database_id))
            .await?;

        if tokio::time::timeout(Duration::from_secs(die.sample(&mut rng)), &mut recv)
            .await
            .is_ok()
        {
            break;
        }
    }
    Ok(())
}

async fn staff(key: String, server: &str, port: u16) -> anyhow::Result<()> {
    let mut conn = SocketConn::connect(server, port)
        .await
        .map_err(|e| anyhow!("Connect teamspeak console error: {:?}", e))?;
    conn.login(&key).await?;

    let (sender, mut receiver) = tokio::sync::oneshot::channel();
    tokio::select! {
        _ = async move {
            tokio::signal::ctrl_c().await.unwrap();
            info!("Recv SIGINT signal, send exit signal");
            sender.send(true).unwrap();
            tokio::signal::ctrl_c().await.unwrap();
            info!("Recv SIGINT again, force exit.");
            std::process::exit(137);
        } => {
            return Ok(())
        }

        _ = async {
            while let Err(e) = conn.who_am_i().await {
                if e.code() == 1794 {
                    if tokio::time::timeout(Duration::from_secs(1), &mut receiver)
                        .await
                        .is_ok()
                    {
                        return Err(e);
                    }
                } else {
                    return Err(e);
                }
            }
            Ok(())
        } => {

        }
    }
    let who_am_i = conn.who_am_i().await?;
    //conn.register_events().await??;

    let variable = conn.query_client_description(who_am_i.client_id()).await?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    //let keepalive_signal = Arc::new(Mutex::new(false));
    tokio::select! {
        _ = async move {
            tokio::signal::ctrl_c().await.unwrap();
            sender.send(true).unwrap();
            info!("Recv SIGINT signal, send exit signal");
            tokio::signal::ctrl_c().await.unwrap();
            info!("Recv SIGINT again, force exit.");
            std::process::exit(137);
        } => {}
        /*_ = async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let mut i = keepalive_signal.lock().await;
                *i = true;
            }
        } => {}*/
        ret = real_staff(conn, receiver, variable) =>  {
           ret?
        }
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .args(&[arg!(<API_KEY> "Api key of client query")])
        .get_matches();

    env_logger::Builder::from_default_env().init();

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(staff(
            matches
                .get_one("API_KEY")
                .map(|s: &String| s.to_string())
                .unwrap(),
            "localhost",
            25639,
        ))?;

    Ok(())
}
