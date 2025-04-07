use std::{fs::File, io::{Read, Write}, path::Path};

#[macro_export]
macro_rules! write_to_file {
    ($data:expr, $dir_name:expr, $file_name:expr) => {{
        let path = Path::new($dir_name).join($file_name);

        let mut file = File::create(&path).expect("Failed to create file");

        let bytes = bincode::serialize(&$data).unwrap();
        file.write_all(&bytes).expect("Failed to write to file");
    }};
}

#[macro_export]
macro_rules! read_from_file {
    ($dir_name:expr, $file_name:expr, $t:ty) => {{
        let path = std::path::Path::new($dir_name).join($file_name);
        let mut file = std::fs::File::open(&path).expect("Failed to open file");
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).expect("Failed to read file");
        let data: $t = bincode::deserialize(&buffer).expect("Failed to deserialize data");
        data
    }};
}

#[macro_export]
macro_rules! serialized_size {
    ($value:expr) => {{
        match bincode::serialized_size(&$value) {
            Ok(size) => size as usize,
            Err(_) => panic!() // Return 0 if there's an error; adjust as needed
        }
    }};
}