use crate::datastructures::{Client, ConnnectInfo, QueryResult, WhoAmI};
use crate::datastructures::{FromQueryString, QueryStatus};
use anyhow::anyhow;
use log::{error, info, warn};
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

    #[allow(dead_code)]
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
    pub async fn disconnect(&mut self) -> QueryResult<()> {
        self.basic_operation("disconnect\n\r").await
    }

    #[allow(dead_code)]
    pub async fn connect_server(
        &mut self,
        address: &str,
        connect_to: &str,
        nick_name: &str,
    ) -> QueryResult<()> {
        self.basic_operation(&format!(
            "connect address={} nickname={} channel={}\n\r",
            address,
            Self::escape(nick_name),
            Self::escape(connect_to)
        ))
        .await
    }

    #[allow(dead_code)]
    pub async fn set_channel_password(&mut self, cid: i64, password: &str) -> QueryResult<()> {
        self.basic_operation(&format!(
            "clientedit cid={} client_password={}\n\r",
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
