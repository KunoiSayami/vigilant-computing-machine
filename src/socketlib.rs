use crate::datastructures::{
    Client, ClientEdit, ClientVariable, ConnectInfo, QueryError, QueryResult, WhoAmI,
};
use crate::datastructures::{FromQueryString, QueryStatus};
use anyhow::anyhow;
use log::{error, warn};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
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
        error!("Should return status in reply => {}", content);
        Err(QueryError::status_not_found())
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
        match Self::decode_status_with_result(data) {
            Ok(ret) => ret,
            Err(e) => {
                if e.code() != -6 {
                    return Err(e);
                }
                Self::decode_status_with_result(self.write_and_read(payload).await?)?
            }
        }
        .ok_or_else(|| QueryError::result_not_found(payload))
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

    #[allow(dead_code)]
    pub async fn logout(&mut self) -> QueryResult<()> {
        self.basic_operation("quit\n\r").await
    }

    #[allow(dead_code)]
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
    pub async fn server_connect_info(&mut self) -> QueryResult<ConnectInfo> {
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

    pub async fn update_client_description(&mut self, edit_var: ClientEdit) -> QueryResult<()> {
        self.basic_operation(&format!(
            "clientdbedit cldbid={} client_description={}\n\r",
            edit_var.client_database_id(),
            Self::escape(edit_var.description())
        ))
        .await
    }

    pub async fn query_client_description(
        &mut self,
        client_id: i64,
    ) -> QueryResult<ClientVariable> {
        self.query_operation(&format!(
            "clientvariable clid={} client_description\n\r",
            client_id
        ))
        .await
        .map(|x| {
            x.map(|mut o| o.remove(0))
                .ok_or_else(|| QueryError::query_error("str"))
        })?
    }
}
