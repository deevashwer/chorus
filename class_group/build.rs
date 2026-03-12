use cmake::Config;
use std::{env, path::PathBuf, process::Command};

fn main() {
    let target = env::var("TARGET").expect("TARGET not found");
    let cplus_header = "c++/src/interface.h";
    let cplus_src = "c++/src/interface.cpp";

    let gmp_path = set_gmp_path();
    println!("gmp path: {}", gmp_path);
    let bicycl_install_prefix: PathBuf;
    if target.contains("android") {
        let ndk = env::var("ANDROID_NDK_HOME").expect("ANDROID_NDK_HOME not found");
        let android_abi = env::var("ANDROID_ABI").expect("ANDROID_ABI not found");
        bicycl_install_prefix = Config::new("c++")
        .define("CMAKE_TOOLCHAIN_FILE", format!("{}/build/cmake/android.toolchain.cmake", ndk))
        .define("ANDROID_ABI", android_abi)
        .define("GMP_DIR", gmp_path.clone())
        .define("GMP_LIBRARY", format!("{}/lib/libgmp.a", gmp_path.clone()))
        .define("GMP_CPP_LIBRARY", format!("{}/lib/libgmp.a", gmp_path.clone()))
        .define("GMP_INC_DIR", format!("{}/include", gmp_path.clone()))
        .define("ANDROID_STL", "c++_shared")
        .build();
    } else {
        bicycl_install_prefix = Config::new("c++")
        .define("GMP_DIR", gmp_path.clone())
        .define("GMP_LIBRARY", format!("{}/lib/libgmp.a", gmp_path.clone()))
        .define("GMP_CPP_LIBRARY", format!("{}/lib/libgmp.a", gmp_path.clone()))
        .define("GMP_INC_DIR", format!("{}/include", gmp_path.clone()))
        .build()
    }
    println!(
        "cargo:rustc-link-search={}/lib",
        bicycl_install_prefix.display()
    );
    println!("cargo:rustc-link-lib=static=bicycl");
    #[cfg(any(target_os = "macos", target_os = "android"))]
    println!("cargo:rustc-link-lib=dylib=c++");
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=dylib=stdc++");

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed={}", cplus_header.to_string());
    println!("cargo:rerun-if-changed={}", cplus_src.to_string());
    println!("cargo:rerun-if-changed=c++/CMakeLists.txt");
    println!("cargo:rerun-if-changed=c++/src/CMakeLists.txt");

    /*
    let gmp_include_path = std::env::var("GMP_INCLUDE_PATH").unwrap_or("/opt/homebrew/include".into());
    // Run bindgen on c++ directory
    let bindings = bindgen::Builder::default()
        .blocklist_file("gmp.h")
        // .allowlist_type("mpz_add_wrapper")
        .header(format!("{}", cplus_header.to_string()))
        .clang_arg(format!("-I{gmp_include_path}"))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to src/bindings.rs
    bindings
        //.write_to_file(project_dir.join("src/bindings.rs"))
        .write_to_file("src/bindings.rs")
        .expect("Couldn't write bindings!");
    */
}

// find path to gmp-mpfr-sys's installation of gmp
fn set_gmp_path() -> String {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let build_dir = PathBuf::from(&out_dir)
        .ancestors()
        .find(|p| p.ends_with("build"))
        .map(|p| p.parent().unwrap().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(&out_dir))
        .display()
        .to_string();
    println!("build dir: {}", build_dir);
    let gmp_search_output = Command::new("find")
    .arg(build_dir)
    .arg("-type")
    .arg("f")
    .arg("-name")
    .arg("gmp.h")
    .output()
    .expect("failed to execute find command");
    let mut gmp_search_output_str =  String::from_utf8(gmp_search_output.stdout)
    .expect("Failed to convert output to string");
    println!("find output: {}", gmp_search_output_str);
    gmp_search_output_str = gmp_search_output_str.split_whitespace().into_iter().collect::<Vec<_>>().pop().unwrap().trim_end_matches("\n").to_string(); // removes ending newline
    println!("search output: {}", gmp_search_output_str);
    format!("{}/../..", gmp_search_output_str) // assumes path to "gmp.h" file is <gmp path>/include/gmp.h
}