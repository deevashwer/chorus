use bincode::de;
use serde::de::value::U8Deserializer;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::collections::HashMap;
use std::net::TcpStream as StdTcpStream;
use std::fmt::format;
use std::io::Read;
use crate::read_from_file;
use crate::secret_recovery::client;
use crate::secret_recovery::common::{CoefficientCommitments, CommitteeData, CommitteeStateClient, Handover, HandoverLite, PublicState, RecoveryRequestBatch, RecoveryResponseBatch};
use ark_std::{cfg_into_iter, end_timer, start_timer};

#[cfg(feature = "parallel")]
use rayon::prelude::*;

const PORT: &str = "32000";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientDownloadContribute {
    pub committee_0: CommitteeData,
    pub committee_1: CommitteeData,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ClientDKGUpload {
    pub handover: Handover,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ClientHandoverUpload {
    pub handover: Handover,
    pub rsp: Option<RecoveryResponseBatch>,
}

#[derive(Debug, Clone)]
pub struct ClientDownloadDKG {
    pub public_state_epoch_2: PublicState,
    pub committee_0: CommitteeData,
    pub committee_1: CommitteeData,
    pub committee_2: CommitteeData,
    pub commstate_1: CommitteeStateClient,
}

pub fn fast_serialize_commstate(commstate: &CommitteeStateClient) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&bincode::serialize(&commstate.epoch).unwrap());
    bytes.extend_from_slice(&bincode::serialize(&commstate.public_state).unwrap());
    bytes.extend_from_slice(&bincode::serialize(&commstate.coeff_cmts).unwrap());
    bytes.extend_from_slice(&bincode::serialize(&commstate.handovers.len()).unwrap());
    let max_handover_size = commstate.handovers.iter().map(|handover| bincode::serialized_size(handover).unwrap()).max().unwrap() as usize;
    bytes.extend_from_slice(&bincode::serialize(&max_handover_size).unwrap());
    for handover in commstate.handovers.iter() {
        let mut handover_bytes = vec![0u8; max_handover_size];
        let serialized = bincode::serialize(&handover).unwrap();
        handover_bytes[..serialized.len()].copy_from_slice(&serialized);

        bytes.extend_from_slice(&handover_bytes);
    }
    bytes
}

pub fn fast_deserialize_commstate(mut cursor: std::io::Cursor<Vec<u8>>) -> (CommitteeStateClient, std::io::Cursor<Vec<u8>>) {
    let epoch: usize = bincode::deserialize_from(&mut cursor).unwrap();
    let public_state: PublicState = bincode::deserialize_from(&mut cursor).unwrap();
    let coeff_cmts: CoefficientCommitments = bincode::deserialize_from(&mut cursor).unwrap();
    let num_handovers: usize = bincode::deserialize_from(&mut cursor).unwrap();
    let max_handover_size: usize = bincode::deserialize_from(&mut cursor).unwrap();
    let mut handover_bytes = vec![0u8; max_handover_size * num_handovers];
    std::io::Read::read_exact(&mut cursor, &mut handover_bytes[..]).unwrap();
    let handovers: Vec<HandoverLite> = cfg_into_iter!(handover_bytes.chunks(max_handover_size).map(|chunk| chunk.to_vec()).collect::<Vec<Vec<u8>>>()).map(|chunk| {
        let handover: HandoverLite = bincode::deserialize_from(&chunk[..]).unwrap();
        handover
    }).collect();

    (CommitteeStateClient {
        epoch,
        public_state,
        coeff_cmts,
        handovers,
    }, cursor)
}

impl Serialize for ClientDownloadDKG {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&bincode::serialize(&self.public_state_epoch_2).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&bincode::serialize(&self.committee_0).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&bincode::serialize(&self.committee_1).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&bincode::serialize(&self.committee_2).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&fast_serialize_commstate(&self.commstate_1));

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for ClientDownloadDKG {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let mut cursor = std::io::Cursor::new(bytes);

        // Deserialize each field in order
        let public_state_epoch_2: PublicState = bincode::deserialize_from(&mut cursor).map_err(serde::de::Error::custom)?;
        let committee_0: CommitteeData = bincode::deserialize_from(&mut cursor).map_err(serde::de::Error::custom)?;
        let committee_1: CommitteeData = bincode::deserialize_from(&mut cursor).map_err(serde::de::Error::custom)?;
        let committee_2: CommitteeData = bincode::deserialize_from(&mut cursor).map_err(serde::de::Error::custom)?;
        let (commstate_1, _) = fast_deserialize_commstate(cursor);

        Ok(Self {
            public_state_epoch_2,
            committee_0,
            committee_1,
            committee_2,
            commstate_1,
        })
    }
}


#[derive(Clone)]
pub struct ClientDownloadHandover {
    pub public_state_epoch_3: PublicState,
    pub committee_1: CommitteeData,
    pub committee_2: CommitteeData,
    pub committee_3: CommitteeData,
    pub commstate_1_coeff_cmts: CoefficientCommitments,
    pub commstate_2: CommitteeStateClient,
    pub reqs_batch: RecoveryRequestBatch,
}

impl Serialize for ClientDownloadHandover {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&bincode::serialize(&self.public_state_epoch_3).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&bincode::serialize(&self.committee_1).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&bincode::serialize(&self.committee_2).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&bincode::serialize(&self.committee_3).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&bincode::serialize(&self.commstate_1_coeff_cmts).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&bincode::serialize(&self.reqs_batch).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&fast_serialize_commstate(&self.commstate_2));

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for ClientDownloadHandover {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let mut cursor = std::io::Cursor::new(bytes);

        // Deserialize each field in order
        let public_state_epoch_3: PublicState = bincode::deserialize_from(&mut cursor).map_err(serde::de::Error::custom)?;
        let committee_1: CommitteeData = bincode::deserialize_from(&mut cursor).map_err(serde::de::Error::custom)?;
        let committee_2: CommitteeData = bincode::deserialize_from(&mut cursor).map_err(serde::de::Error::custom)?;
        let committee_3: CommitteeData = bincode::deserialize_from(&mut cursor).map_err(serde::de::Error::custom)?;
        let commstate_1_coeff_cmts: CoefficientCommitments = bincode::deserialize_from(&mut cursor).map_err(serde::de::Error::custom)?;
        let reqs_batch: RecoveryRequestBatch = bincode::deserialize_from(&mut cursor).map_err(serde::de::Error::custom)?;
        let (commstate_2, _) = fast_deserialize_commstate(cursor);

        Ok(Self {
            public_state_epoch_3,
            committee_1,
            committee_2,
            committee_3,
            commstate_1_coeff_cmts,
            commstate_2,
            reqs_batch,
        })
    }
}


pub struct ServerDownloadContribute {
    pub committee_0: CommitteeData,
    pub committee_1: CommitteeData,
}

fn human_readable_format(num: usize) -> String {
    if num >= 1_000_000_000 {
        format!("{:.0}B", num as f64 / 1_000_000_000.0)
    } else if num >= 1_000_000 {
        format!("{:.0}M", num as f64 / 1_000_000.0)
    } else if num >= 1_000 {
        format!("{:.0}K", num as f64 / 1_000.0)
    } else {
        num.to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub enum API {
    DKGContribute,
    DKGHandover,
    TypicalHandover,
}

const CASES: [usize; 2] = [1, 2];
const NUM_CLIENTS: [usize; 3] = [
    10usize.pow(6),
    10usize.pow(7),
    10usize.pow(8)
];

pub type NetworkResponses = HashMap<NetworkRequest, Vec<u8>>;

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct NetworkRequest {
    pub case: usize,
    pub num_clients: usize,
    pub api: API,
}

impl NetworkRequest {
    pub fn dir_name(&self) -> String {
        format!("case_{}_clients_{}", self.case, human_readable_format(self.num_clients))
    }

    pub const fn bytesize() -> usize {
        std::mem::size_of::<usize>() * 2 + std::mem::size_of::<API>() + 3
    }

    pub fn response(&self, responses: &NetworkResponses) -> Vec<u8> {
        let response: Vec<u8> = match self.api {
            API::DKGContribute => {
                let dir_name = self.dir_name();
                let committee_0 = read_from_file!(&dir_name, "committee_0", CommitteeData);
                let committee_1 = read_from_file!(&dir_name, "committee_1", CommitteeData);
                let client_download = ClientDownloadContribute {
                    committee_0,
                    committee_1,
                };
                bincode::serialize(&client_download).unwrap()
            }
            API::DKGHandover => {
                let dir_name = self.dir_name();
                let public_state_epoch_2 = read_from_file!(&dir_name, "public_state_epoch_2", PublicState);
                let committee_0 = read_from_file!(&dir_name, "committee_0", CommitteeData);
                let committee_1 = read_from_file!(&dir_name, "committee_1", CommitteeData);
                let committee_2 = read_from_file!(&dir_name, "committee_2", CommitteeData);
                let commstate_1 = read_from_file!(&dir_name, "commstate_1_seat_idx", CommitteeStateClient);
                let client_download = ClientDownloadDKG {
                    public_state_epoch_2,
                    committee_0,
                    committee_1,
                    committee_2,
                    commstate_1,
                };
                bincode::serialize(&client_download).unwrap()
            }
            API::TypicalHandover => {
                let dir_name = self.dir_name();
                let public_state_epoch_3 = read_from_file!(&dir_name, "public_state_epoch_3", PublicState);
                let committee_1 = read_from_file!(&dir_name, "committee_1", CommitteeData);
                let committee_2 = read_from_file!(&dir_name, "committee_2", CommitteeData);
                let committee_3 = read_from_file!(&dir_name, "committee_3", CommitteeData);
                let commstate_1 = read_from_file!(&dir_name, "commstate_1_seat_idx", CommitteeStateClient);
                let commstate_2 = read_from_file!(&dir_name, "commstate_2_seat_idx", CommitteeStateClient);
                let reqs_batch = read_from_file!(&dir_name, "requests_batch", RecoveryRequestBatch);
                let commstate_1_coeff_cmts = commstate_1.coeff_cmts.clone();
                let client_download = ClientDownloadHandover {
                    public_state_epoch_3,
                    committee_1,
                    committee_2,
                    committee_3,
                    commstate_1_coeff_cmts,
                    commstate_2,
                    reqs_batch,
                };
                bincode::serialize(&client_download).unwrap()
            }
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&response.len().to_le_bytes());
        bytes.extend_from_slice(&response);
        bytes
    }
}

pub async fn populate_network_responses() -> NetworkResponses {
    let mut responses = HashMap::new();
    for case in CASES.iter() {
        for api in [API::DKGContribute, API::DKGHandover, API::TypicalHandover].iter() {
            for num_clients in NUM_CLIENTS.iter() {
                let request = NetworkRequest {
                    case: *case,
                    num_clients: *num_clients,
                    api: api.clone(),
                };
                let response = request.response(&responses);
                responses.insert(request, response);
            }
        }
    }
    responses
}

pub async fn network_service() -> Result<(), Box<dyn std::error::Error>> {
    let ip = format!("0.0.0.0:{}", PORT);
    let addr = ip.parse().unwrap();

    let network_responses = populate_network_responses().await;

    let socket = TcpSocket::new_v4()?;
    socket.set_nodelay(true)?;
    socket.set_send_buffer_size(1024 * 1024)?; // 1MB
    socket.set_recv_buffer_size(1024 * 1024)?; // 1MB
    println!("socket.send_buffer_size: {}", socket.send_buffer_size()?);
    println!("socket.recv_buffer_size: {}", socket.recv_buffer_size()?);
    socket.set_reuseaddr(true)?;
    socket.bind(addr)?;

    let listener = socket.listen(1024)?;
    // let listener = TcpListener::bind(&ip).await?;
    println!("Server listening on {}", ip);

    loop {
        let (mut stream, _) = listener.accept().await?;
        stream.set_nodelay(true).unwrap();
        let network_responses_clone = network_responses.clone();
        tokio::spawn(async move {
            let mut buffer = [0; NetworkRequest::bytesize()];

            if let Ok(n) = stream.read_exact(&mut buffer).await {
                if n == 0 {
                    return;
                }
                assert!(n == NetworkRequest::bytesize());

                let request: NetworkRequest = bincode::deserialize(&buffer).unwrap();

                let process_request = ark_std::start_timer!(|| "Processing Request");
                let response: Vec<u8> = match request.clone() {
                    NetworkRequest { case, num_clients, api } => {
                        println!("Received request: case {}, num_clients {}, api: {:?}", case, num_clients, api);
                        network_responses_clone.get(&request).unwrap().clone()
                    }
                };
                ark_std::end_timer!(process_request);

                let write_response = ark_std::start_timer!(|| "Sending Response");
                // Send the response as bytes
                let _ = stream.write_all(&response).await.unwrap();
                stream.flush().await.unwrap();
                ark_std::end_timer!(write_response);
            }

            // receive the processed data from it and if it matches the expected data, send an ack
            let mut buffer = [0; 8];
            match stream.read_exact(&mut buffer).await {
                Ok(n) => {
                    if n != 8 {
                        return;
                    }
                    let data_length = usize::from_le_bytes(buffer) as usize;

                    // Allocate a buffer for the data
                    let mut data = vec![0u8; data_length];
                    if let Ok(n) = stream.read_exact(&mut data).await {
                        if n != data_length {
                            return;
                        }
                        let ack = 1usize;
                        let _ = stream.write_all(ack.to_le_bytes().as_slice()).await;
                        stream.flush().await.unwrap();
                        println!("ACK sent");
                    };
                }
                _ => return,
            };
        });
    }
}

pub async fn download_from_network<T: DeserializeOwned>(ip: &str, request: &NetworkRequest) -> Result<(T, TcpStream), Box<dyn std::error::Error>> {
    let ip = format!("{}:{}", ip, PORT);

    let addr = ip.parse().unwrap();
    let socket = TcpSocket::new_v4()?;
    socket.set_nodelay(true)?;
    // socket.set_send_buffer_size(1024 * 1024)?; // 1MB
    // socket.set_recv_buffer_size(1024 * 1024)?; // 1MB
    // println!("socket.send_buffer_size: {}", socket.send_buffer_size()?);
    // println!("socket.recv_buffer_size: {}", socket.recv_buffer_size()?);
    let mut stream = socket.connect(addr).await?;
    stream.set_nodelay(true)?;

    let request_bytes = bincode::serialize(&request).unwrap();
    assert!(request_bytes.len() == NetworkRequest::bytesize());
    stream.write_all(&request_bytes).await?;
    stream.flush().await?;

    // Read the response bytes
    let mut buffer = [0; 8]; // To read the data size

    // Read the first 4 bytes for the size
    let data: Vec<u8> = match stream.read_exact(&mut buffer).await {
        Ok(n) => {
            if n != 8 {
                return Err("Invalid data size".into());
            }
            // let data_length_read = ark_std::start_timer!(|| "Reading Data Length");
            let data_length = usize::from_le_bytes(buffer) as usize;
            // ark_std::end_timer!(data_length_read);

            // let data_read = ark_std::start_timer!(|| "Reading Data");
            // Allocate a buffer for the data
            let mut data = vec![0u8; data_length];
            let n = stream.read_exact(&mut data).await?;
            if n != data_length {
                return Err("Failed to read data".into());
            }
            // ark_std::end_timer!(data_read);
            Ok::<Vec<u8>, Box<dyn std::error::Error>>(data)
        }
        Err(e) => {
            eprintln!("Failed to read data size: {:?}", e);
            return Err(e.into());
        }
    }?;

    let deserialization = ark_std::start_timer!(|| "Deserializing Data");
    let downloaded: T = bincode::deserialize(&data).unwrap();
    ark_std::end_timer!(deserialization);
    Ok((downloaded, stream))
}

pub async fn upload_to_network<T: Serialize>(stream: &mut TcpStream, response: &T) -> Result<(), Box<dyn std::error::Error>> {
    let response_bytes = bincode::serialize(&response).unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&response_bytes.len().to_le_bytes());
    bytes.extend_from_slice(&response_bytes);
    stream.write_all(&bytes).await?;
    stream.flush().await?;

    let mut buffer = [0; 8]; 
    match stream.read_exact(&mut buffer).await {
        Ok(n) => {
            if n != 8 {
                return Err("Invalid data size".into());
            }
            let ack = usize::from_le_bytes(buffer) as usize;
            if ack != 1 {
                return Err("Failed to receive ack".into());
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        Err(e) => {
            eprintln!("Failed to read data size: {:?}", e);
            return Err(e.into());
        }
    }?;
    Ok(())
}
