use std::collections::HashMap;
use serde_json::{json, Value, from_slice, to_vec};
use log::*;
use tokio::{io::AsyncWriteExt, fs::File, sync::mpsc::Receiver};
use streaming_platform::{ServerConfig, stream_mode, tokio::{self, runtime::Runtime, io::AsyncReadExt}, DATA_BUF_SIZE, MagicBall, ClientMsg, RestreamMsg, StreamLayout, StreamUnit, sp_dto::{MsgMeta, MsgKind, reply_to_rpc_dto2_sizes, rpc_dto_with_correlation_id_sizes, Route, Participator, RouteSpec, uuid::Uuid, RpcResult}};

mod cfg;

struct FileStreamLayout {
    stream: StreamLayout,
    payload: Option<Value>,
    download_payload: Option<Value>,
    file_id: Option<Uuid>,
    file: Option<File>,
    rpc_result: RpcResult
}

fn main() {
    env_logger::init();
    let config = cfg::get_config();    
    let access_key = "";
    let mut rt = Runtime::new().expect("failed to create runtime");
    let mut hm_config = HashMap::new();
    hm_config.insert("access_key".to_owned(), config.access_key.clone());
    rt.block_on(stream_mode(&config.host, &config.addr, access_key, process_stream, startup, hm_config, None));
}

pub async fn startup(config: HashMap<String, String>, mut mb: MagicBall) {
    let access_key = config.get("access_key").expect("access key is empty");
    let (correlation_id, dto, msg_meta_size, payload_size, attachments_sizes) = rpc_dto_with_correlation_id_sizes(
        mb.addr.clone(),
        "File".to_owned(),
        "Download".to_owned(),
        json!({
            "access_key": access_key
        }),
        Route {
            source: Participator::Service(mb.addr.clone()),
            spec: RouteSpec::Simple,
            points: vec![Participator::Service(mb.addr.clone())]
        }
    ).expect("failed to create download rpc dto");    
    let stream_id = mb.get_stream_id();
    mb.write_vec(
        stream_id,
        dto, 
        msg_meta_size, 
        payload_size, 
        attachments_sizes
    ).await.expect("failed to write download rpc dto");
}

pub async fn process_stream(config: HashMap<String, String>, mut mb: MagicBall, mut rx: Receiver<ClientMsg>, _: Option<Receiver<RestreamMsg>>) {
    let storage_path = config.get("storage_path").expect("missing storage_path config value");
    let mut stream_layouts = HashMap::new();    
    loop {        
        let client_msg = rx.recv().await.expect("connection issues acquired");
        let stream_id = client_msg.get_stream_id();
        match process_client_msg(&mut mb, &mut stream_layouts, storage_path, client_msg).await {
            Ok(()) => {}
            Err(e) => {
                match stream_id {
                    Some(stream_id) => {
                        match stream_layouts.remove(&stream_id) {
                            Some(stream_layout) => {
                                match stream_layout.stream.msg_meta.kind {
                                    MsgKind::RpcRequest => {
                                        let mut route = stream_layout.stream.msg_meta.route.clone();
                                        route.points.push(Participator::Service(mb.addr.clone()));
                                        let (res, msg_meta_size, payload_size, attachments_size) = reply_to_rpc_dto2_sizes(
                                            mb.addr.clone(), 
                                            stream_layout.stream.msg_meta.tx.clone(), 
                                            stream_layout.stream.msg_meta.key.clone(), 
                                            stream_layout.stream.msg_meta.correlation_id, 
                                            vec![],
                                            vec![], vec![],
                                            RpcResult::Err,
                                            route
                                        ).expect("failed to create rpc reply");
                                        mb.write_vec(stream_layout.stream.id, res, msg_meta_size, payload_size, attachments_size).await.expect("failed to write response to upload");
                                    }
                                    _ => {}
                                }                        
                            }
                            None => {}
                        }
                        error!("{:?}", e);
                    }
                    None => {}
                }
            }
        }
    }
}

async fn process_client_msg(mb: &mut MagicBall, stream_layouts: &mut HashMap<u64, FileStreamLayout>, storage_path: &str, client_msg: ClientMsg) -> Result<(), Error> {
    match client_msg {
        ClientMsg::MsgMeta(stream_id, msg_meta) => {            
            stream_layouts.insert(stream_id, FileStreamLayout {
                stream: StreamLayout {
                    id: stream_id,
                    msg_meta,
                    payload: vec![],
                    attachments_data: vec![]
                },
                payload: None,
                download_payload: None,
                file_id: None,
                file: None,
                rpc_result: RpcResult::Ok
            });
        }
        ClientMsg::PayloadData(stream_id, n, buf) => {
            let stream_layout = stream_layouts.get_mut(&stream_id).ok_or(Error::CustomError("not found stream for payload data".to_owned()))?;
            stream_layout.stream.payload.extend_from_slice(&buf[..n]);            
        }
        ClientMsg::PayloadFinished(stream_id, n, buf) => {
            let stream_layout = stream_layouts.get_mut(&stream_id).ok_or(Error::CustomError("not found stream for payload finish".to_owned()))?;
            stream_layout.stream.payload.extend_from_slice(&buf[..n]);
            match stream_layout.stream.msg_meta.key.as_ref() {
                "Upload" => {
                    let attachment = stream_layout.stream.msg_meta.attachments.iter().nth(0).ok_or(Error::CustomError("no attachment found in msg meta for upload key".to_owned()))?;
                    let payload: Value = from_slice(&stream_layout.stream.payload)?;                    
                    let mut path = String::new();                    
                    stream_layout.file = Some(File::create(path).await?);
                    stream_layout.payload = Some(payload);
                }
                "Download" => {
                    let payload: Value = from_slice(&stream_layout.stream.payload)?;
                    stream_layout.download_payload = Some(payload);                        
                }
                _ => {}
            }                        
        }
        ClientMsg::AttachmentData(stream_id, index, n, buf) => {
            let stream_layout = stream_layouts.get_mut(&stream_id).ok_or(Error::CustomError("not found stream for attachment data".to_owned()))?;
            match stream_layout.stream.msg_meta.key.as_ref() {
                "Upload" => {
                    let file = stream_layout.file.as_mut().ok_or(Error::CustomError("file is empty for attachment data".to_owned()))?;
                    file.write_all(&buf[..n]).await?;
                }
                _ => {}
            }                                                
        }
        ClientMsg::AttachmentFinished(stream_id, index, n, buf) => {
            let stream_layout = stream_layouts.get_mut(&stream_id).ok_or(Error::CustomError("not found stream for attachment finish".to_owned()))?;
            match stream_layout.stream.msg_meta.key.as_ref() {
                "Upload" => {
                    let file = stream_layout.file.as_mut().ok_or(Error::CustomError("file is empty for attachment data".to_owned()))?;
                    file.write_all(&buf[..n]).await?;
                    stream_layout.file = None;
                }
                _ => {}
            }                                        
        }
        ClientMsg::MessageFinished(stream_id) => {                                
            let stream_layout = stream_layouts.remove(&stream_id).ok_or(Error::CustomError("not found stream for message finish".to_owned()))?;
            match stream_layout.stream.msg_meta.key.as_ref() {                            
                "Upload" => {                    
                    let file_id = stream_layout.file_id.ok_or(Error::CustomError("empty file id for upload message finish".to_owned()))?;
                    let payload = stream_layout.payload.ok_or(Error::CustomError("empty payload for upload message finish".to_owned()))?;                                        
                    let reponse_payload = to_vec(&json!({

                    }))?;
                    let mut route = stream_layout.stream.msg_meta.route.clone();
                    route.points.push(Participator::Service(mb.addr.clone()));
                    let (res, msg_meta_size, payload_size, attachments_size) = reply_to_rpc_dto2_sizes(
                        mb.addr.clone(), 
                        stream_layout.stream.msg_meta.tx.clone(), 
                        stream_layout.stream.msg_meta.key.clone(), 
                        stream_layout.stream.msg_meta.correlation_id, 
                        reponse_payload,
                        vec![], vec![],
                        stream_layout.rpc_result.clone(),
                        route
                    )?;
                    mb.write_vec(stream_layout.stream.id, res, msg_meta_size, payload_size, attachments_size).await.expect("failed to write response to upload");
                }
                "Download" => {
                    let FileStreamLayout { 
                        stream,
                        payload: _,
                        download_payload,
                        file_id: _,
                        file: _,
                        rpc_result: _
                    } = stream_layout;
                    let payload = download_payload.ok_or(Error::CustomError("empty download payload for download message finish".to_owned()))?;
                    let service_path = payload["service_path"].as_str().ok_or(Error::OptionIsNone)?;
                    let domain_path = payload["domain_path"].as_str().ok_or(Error::OptionIsNone)?;
                    let token_path = payload["token_path"].as_str().ok_or(Error::OptionIsNone)?;
                    let file_id = payload["file_id"].as_str().ok_or(Error::OptionIsNone)?;
                    let file_name = payload["file_name"].as_str().ok_or(Error::OptionIsNone)?.to_owned();
                    let mut path = String::new();
                    path.push_str(&storage_path);
                    path.push_str("/");
                    path.push_str(service_path);
                    path.push_str("/");
                    path.push_str(domain_path);
                    path.push_str("/");
                    path.push_str(token_path);
                    path.push_str("/");
                    path.push_str(file_id);
                    let mb = mb.clone();
                    tokio::spawn(async move {                    
                        match download_file(mb, stream.msg_meta, path, file_name.clone()).await {
                            Ok(()) => {
                                info!("file download complete, name {}", file_name);
                            }
                            Err(e) => {
                                error!("download file error {:?}", e);
                            }
                        }
                    });                    
                }
                _ => {}
            }
        }
        _ => {}
    }
    Ok(())
}

async fn download_file(mut mb: MagicBall, msg_meta: MsgMeta, path: String, file_name: String) -> Result<(), Error> {
    let mut file = File::open(&path).await?;
    let size = file.metadata().await?.len();
    let (dto, msg_meta_size, payload_size, _) = reply_to_rpc_dto2_sizes(mb.addr.clone(), msg_meta.tx.clone(), msg_meta.key.clone(), msg_meta.correlation_id, vec![], vec![(file_name, size)], vec![], RpcResult::Ok, msg_meta.route.clone())?;    
    let stream_id = mb.get_stream_id();
    mb.write_vec(stream_id, dto, msg_meta_size, payload_size, vec![]).await?;        
    match size {
        0 => {
            mb.write_tx.send(StreamUnit::Empty(stream_id)).await?
        }
        _ => {
            let mut file_buf = [0; DATA_BUF_SIZE];
            loop {
                match file.read(&mut file_buf).await? {
                    0 => break,
                    n => {                
                        mb.write_tx.send(StreamUnit::Array(stream_id, n, file_buf)).await?
                    }
                }
            }
        }
    }        
    Ok(())
}

#[derive(Debug)]
pub enum Error {    
	Io(std::io::Error),	
    SerdeJson(serde_json::Error),
    StreamingPlatform(streaming_platform::ProcessError),
    SendStreamUnit,
    OptionIsNone,
    CustomError(String)
}

impl From<std::io::Error> for Error {
	fn from(e: std::io::Error) -> Error {
		Error::Io(e)
	}
}

impl From<serde_json::Error> for Error {
	fn from(e: serde_json::Error) -> Error {
		Error::SerdeJson(e)
	}
}

impl From<streaming_platform::ProcessError> for Error {
    fn from(e: streaming_platform::ProcessError) -> Error {
        Error::StreamingPlatform(e)
    }
}

impl From<tokio::sync::mpsc::error::SendError<streaming_platform::StreamUnit>> for Error {
    fn from(e: tokio::sync::mpsc::error::SendError<streaming_platform::StreamUnit>) -> Error {
        Error::SendStreamUnit
    }
}




/*
fn process_msg(msg: ClientMsg, save_path: String) {
    match msg {
        ClientMsg::FileReceiveComplete(name) => unpack(save_path, name),
        _ => {}
    }
}
*/