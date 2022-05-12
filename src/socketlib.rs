use crate::datastructures::{Channel, Client, ConnnectInfo, QueryError, QueryResult, WhoAmI};
use crate::datastructures::{FromQueryString, QueryStatus};
use anyhow::anyhow;
use log::{error, info, warn};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::error::Elapsed;
const BUFFER_SIZE: usize = 512;

pub struct SocketConn {
    conn: TcpStream,
}

impl SocketConn {
    async fn read_data(&mut self) -> anyhow::Result<Option<String>> {
        let mut buffer = [0u8; BUFFER_SIZE];
        let mut ret = String::new();
        loop {
            let size = if let Ok(data) =
                tokio::time::timeout(Duration::from_secs(2), self.conn.read(&mut buffer)).await
            {
                match data {
                    Ok(size) => size,
                    Err(e) => return Err(anyhow!("Got error while read data: {:?}", e)),
                }
            } else {
                return Ok(None);
            };

            ret.push_str(&String::from_utf8_lossy(&buffer[..size]));
            if size < BUFFER_SIZE
                || (ret
                    .lines()
                    .into_iter()
                    .any(|line| line.starts_with("error id=")))
            {
                break;
            }
        }
        Ok(Some(ret))
    }

    async fn write_data(&mut self, payload: &str) -> anyhow::Result<()> {
        debug_assert!(payload.ends_with("\n\r"));
        self.conn
            .write(payload.as_bytes())
            .await
            .map(|size| {
                if size != payload.as_bytes().len() {
                    error!(
                        "Error payload size mismatch! expect {} but {} found. payload: {:?}",
                        payload.as_bytes().len(),
                        size,
                        payload
                    )
                }
            })
            .map_err(|e| anyhow!("Got error while send data: {:?}", e))?;
        Ok(())
    }
    fn decode_status(content: String) -> QueryResult<String> {
        /*debug_assert!(
            !content.contains("Welcome to the TeamSpeak 3") && content.contains("error id="),
            "Content => {:?}",
            content
        );*/

        for line in content.lines() {
            if line.trim().starts_with("error ") {
                let status = QueryStatus::try_from(line)?;

                return status.into_result(content);
            }
        }
        panic!("Should return status in reply => {}", content)
    }

    fn decode_status_with_result<T: FromQueryString + Sized>(
        data: String,
    ) -> QueryResult<Option<Vec<T>>> {
        let content = Self::decode_status(data)?;

        for line in content.lines() {
            if !line.starts_with("error ") {
                let mut v = Vec::new();
                for element in line.split('|') {
                    v.push(T::from_query(element)?);
                }
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    async fn delay_read(&mut self) -> anyhow::Result<String> {
        let mut s = String::new();
        loop {
            let r = self
                .read_data()
                .await?
                .ok_or_else(|| anyhow!("READ NONE DATA"))?;
            s.push_str(&r);
            if s.lines().any(|line| line.trim().starts_with("error id=")) {
                break;
            }
        }
        Ok(s)
    }

    async fn write_and_read(&mut self, payload: &str) -> anyhow::Result<String> {
        self.write_data(payload).await?;
        self.delay_read().await
    }

    async fn basic_operation(&mut self, payload: &str) -> QueryResult<()> {
        let data = self.write_and_read(payload).await?;
        Self::decode_status(data).map(|_| ())
    }

    async fn query_operation_non_error<T: FromQueryString + Sized>(
        &mut self,
        payload: &str,
    ) -> QueryResult<Vec<T>> {
        let data = self.write_and_read(payload).await?;
        let ret = Self::decode_status_with_result(data)?;
        Ok(ret
            .ok_or_else(|| panic!("Can't find result line, payload => {}", payload))
            .unwrap())
    }

    #[allow(dead_code)]
    async fn query_operation<T: FromQueryString + Sized>(
        &mut self,
        payload: &str,
    ) -> QueryResult<Option<Vec<T>>> {
        let data = self.write_and_read(payload).await?;
        Self::decode_status_with_result(data)
        //let status = status.ok_or_else(|| anyhow!("Can't find status line."))?;
    }

    fn escape(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace(' ', "\\s")
            .replace('/', "\\/")
    }

    pub async fn connect(server: &str, port: u16) -> anyhow::Result<Self> {
        let conn = TcpStream::connect(format!("{}:{}", server, port))
            .await
            .map_err(|e| anyhow!("Got error while connect to {}:{} {:?}", server, port, e))?;

        //let bufreader = BufReader::new(conn);
        //conn.set_nonblocking(true).unwrap();
        let mut self_ = Self { conn };

        tokio::time::sleep(Duration::from_millis(10)).await;
        let content = self_
            .read_data()
            .await
            .map_err(|e| anyhow!("Got error in connect while read content: {:?}", e))?;

        if content.is_none() {
            warn!("Read none data.");
        }

        Ok(self_)
    }

    pub async fn login(&mut self, key: &str) -> QueryResult<()> {
        let payload = format!("auth apikey={}\n\r", key);
        self.basic_operation(payload.as_str()).await
    }

    #[allow(dead_code)]
    pub async fn who_am_i(&mut self) -> QueryResult<WhoAmI> {
        self.query_operation_non_error("whoami\n\r")
            .await
            .map(|mut v| v.remove(0))
    }

    pub async fn query_clients(&mut self) -> QueryResult<Vec<Client>> {
        self.query_operation_non_error("clientlist\n\r").await
    }

    pub async fn query_channels(&mut self) -> QueryResult<Vec<Channel>> {
        self.query_operation_non_error("channellist\n\r").await
    }

    #[allow(dead_code)]
    pub async fn logout(&mut self) -> QueryResult<()> {
        self.basic_operation("quit\n\r").await
    }

    #[allow(dead_code)]
    pub async fn disconnect(&mut self) -> QueryResult<()> {
        self.basic_operation("disconnect\n\r").await
    }

    #[allow(dead_code)]
    pub async fn connect_server(
        &mut self,
        address: &str,
        //connect_to: &str,
        nick_name: &str,
    ) -> QueryResult<()> {
        self.basic_operation(&format!(
            "connect address={} nickname={}\n\r",
            address,
            Self::escape(nick_name),
        ))
        .await
    }

    pub async fn set_current_channel_password(&mut self, password: &str) -> QueryResult<()> {
        let me = self.who_am_i().await?;
        self.set_channel_password(me.channel_id(), password).await
    }

    pub async fn set_channel_password(&mut self, cid: i64, password: &str) -> QueryResult<()> {
        self.basic_operation(&format!(
            "channeledit cid={} channel_password={}\n\r",
            cid,
            Self::escape(password)
        ))
        .await
    }

    #[allow(dead_code)]
    pub async fn server_connect_info(&mut self) -> QueryResult<ConnnectInfo> {
        self.query_operation_non_error("serverconnectinfo\n\r")
            .await
            .map(|mut v| v.remove(0))
    }

    #[allow(dead_code)]
    pub async fn switch_channel(&mut self, channel_id: i64) -> QueryResult<()> {
        let me = self.who_am_i().await?;
        self.basic_operation(&format!(
            "clientmove cid={} clid={}\n\r",
            channel_id,
            me.client_id(),
        ))
        .await
    }

    pub async fn wait_until_connect(&mut self) -> QueryResult<()> {
        while let Err(e) = self.who_am_i().await {
            if e.code() == 1794 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            } else {
                return Err(e);
            }
        }
        Ok(())
    }

    pub async fn wait_timeout(&mut self, duration: Duration) -> Result<QueryResult<()>, Elapsed> {
        tokio::time::timeout(duration, self.wait_until_connect()).await
    }

    pub async fn switch_channel_by_name(&mut self, channel_name: &str) -> QueryResult<()> {
        let me = self.who_am_i().await?;
        for channel in self
            .query_channels()
            .await
            .map_err(|e| anyhow!("Can't fetch channels: {:?}", e))?
        {
            if channel.channel_name().eq(channel_name) {
                return self
                    .basic_operation(&format!(
                        "clientmove cid={} clid={}\n\r",
                        channel.cid(),
                        me.client_id(),
                    ))
                    .await;
            }
        }
        Err(QueryError::channel_not_found())
    }

    pub async fn check_self_duplicate(&mut self) -> QueryResult<bool> {
        let my = self.who_am_i().await?;
        let clients = self.query_clients().await?;
        let mut database_id = 0;
        for client in &clients {
            if client.client_id() == my.client_id() {
                database_id = client.client_database_id();
            }
        }
        if database_id == 0 {
            return Err(QueryError::database_id_error());
        }

        Ok(clients
            .iter()
            .filter(|&n| n.client_database_id() == database_id)
            .count()
            > 1)
    }

    #[allow(dead_code)]
    pub async fn check_self_duplicate_with_id(&mut self, database_id: i64) -> QueryResult<bool> {
        Ok(self
            .query_clients()
            .await?
            .iter()
            .filter(|&n| n.client_database_id() == database_id)
            .count()
            > 1)
    }

    pub async fn query_database_id(&mut self) -> QueryResult<i64> {
        let my = self.who_am_i().await?;
        let mut database_id = 0;
        for client in self
            .query_clients()
            .await
            .map_err(|e| anyhow!("Got error while query clients: {:?}", e))?
        {
            if client.client_id() == my.client_id() {
                database_id = client.client_database_id();
            }
        }
        if database_id == 0 {
            return Err(QueryError::database_id_error());
        }
        Ok(database_id)
    }
}

pub struct VLCConn {
    conn: TcpStream,
}

impl VLCConn {
    async fn read_data(&mut self) -> anyhow::Result<Option<String>> {
        let mut buffer = [0u8; BUFFER_SIZE];
        let mut ret = String::new();
        loop {
            let size = if let Ok(data) =
                tokio::time::timeout(Duration::from_secs(2), self.conn.read(&mut buffer)).await
            {
                match data {
                    Ok(size) => size,
                    Err(e) => return Err(anyhow!("Got error while read data: {:?}", e)),
                }
            } else {
                return Ok(None);
            };

            ret.push_str(&String::from_utf8_lossy(&buffer[..size]));
            if size < BUFFER_SIZE || (ret.contains("error id=") && ret.ends_with("\n\r")) {
                break;
            }
        }
        Ok(Some(ret))
    }

    async fn write_data(&mut self, payload: &str) -> anyhow::Result<()> {
        debug_assert!(payload.ends_with("\n\r"));
        self.conn
            .write(payload.as_bytes())
            .await
            .map(|size| {
                if size != payload.as_bytes().len() {
                    error!(
                        "Error payload size mismatch! expect {} but {} found. payload: {:?}",
                        payload.as_bytes().len(),
                        size,
                        payload
                    )
                }
            })
            .map_err(|e| anyhow!("Got error while send data: {:?}", e))?;
        Ok(())
    }
    pub async fn connect(server: &str, port: u16, password: &str) -> anyhow::Result<Self> {
        let conn = TcpStream::connect(format!("{}:{}", server, port))
            .await
            .map_err(|e| anyhow!("Got error while connect to {}:{} {:?}", server, port, e))?;

        let mut self_ = Self { conn };

        tokio::time::sleep(Duration::from_millis(10)).await;
        let content = self_
            .read_data()
            .await
            .map_err(|e| anyhow!("Got error in connect while read content: {:?}", e))?
            .unwrap();

        if content.contains("Password") {
            self_
                .write_data(password)
                .await
                .map_err(|e| anyhow!("Got error while write password: {:?}", e))?;
        }

        let content = self_
            .read_data()
            .await
            .map_err(|e| anyhow!("Got error in verify password: {:?}", e))?
            .unwrap();

        if content.contains("Welcome, Master") {
            info!("Login successful");
        } else {
            return Err(anyhow!("Wrong password!"));
        }

        Ok(self_)
    }

    pub async fn get_status(&mut self) -> anyhow::Result<bool> {
        self.write_data("status\n\r")
            .await
            .map_err(|e| anyhow!("Got error while write VLC status: {:?}", e))?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        let content = self
            .read_data()
            .await
            .map_err(|e| anyhow!("Got error while read VLC status: {:?}", e))?
            .ok_or_else(|| anyhow!("VLC return data is None"))?;

        if !content.contains("( state") {
            return Err(anyhow!("Return value not include VLC status"));
        }

        for line in content.lines() {
            if line.contains("( state") {
                return Ok(line.contains("playing )"));
            }
        }
        Ok(false)
    }

    pub async fn play(&mut self) -> anyhow::Result<()> {
        self.write_data("play\n\r").await
    }

    pub async fn pause(&mut self) -> anyhow::Result<()> {
        self.write_data("pause\n\r").await
    }
}
