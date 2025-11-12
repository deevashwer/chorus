use ark_std::{start_timer, end_timer};
use chorus::network::{download_from_network, upload_to_network, ClientDownloadContribute, ClientDownloadDKG, ClientDownloadHandover, NetworkRequest, API};

const NETWORK_IP_FOR_CLIENTS: &str = "34.47.206.12";

#[tokio::main]
async fn main() {
    let request = NetworkRequest {
        case: 1,
        num_clients: 10usize.pow(6),
        api: API::TypicalHandover,
    };
    #[cfg(feature = "print-trace")]
    let download_time = start_timer!(|| "download for handover-dkg");
    let (download, mut stream) = download_from_network::<ClientDownloadHandover>(&NETWORK_IP_FOR_CLIENTS, &request).await.unwrap();
    #[cfg(feature = "print-trace")]
    end_timer!(download_time);
    #[cfg(feature = "print-trace")]
    let upload_time = start_timer!(|| "upload for handover-dkg");
    upload_to_network::<ClientDownloadHandover>(&mut stream, &download).await.unwrap();
    #[cfg(feature = "print-trace")]
    end_timer!(upload_time);
    /*
    let ip = "0.0.0.0";
    let request = NetworkRequest {
        case: 0,
        num_clients: 10usize.pow(5),
        api: API::TypicalHandover,
    };
    let (response, mut stream) = download_from_network::<ClientDownloadHandover>(&ip, &request).await.unwrap();
    println!("Downloaded bytes: {}", bincode::serialized_size(&response).unwrap());
    let data = bincode::serialize(&response).unwrap();
    upload_to_network::<Vec<u8>>(&mut stream, &data).await.unwrap();
    println!("Upload done");
    */
}