use std::fs;

pub mod globals {
    pub const TEST_PORT: u16 = 6665;
    pub const COMMITTEE_SIZE: u32 = 200;
    pub const NUM_CLIENTS: u32 = 5000;
}

pub fn get_peak_memory_usage() -> usize {
    let status = fs::read_to_string("/proc/self/status").unwrap();
    status
        .lines()
        .find(|line| line.starts_with("VmHWM:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .map(|kb| kb.parse::<usize>().unwrap() * 1024)
        .unwrap()
}

// #[cfg(target_os = "android")]
pub fn get_battery_info_via_dumpsys() -> Option<(i64, i64, i64)> {
    // Execute `adb shell dumpsys battery` and capture output
    let output = std::process::Command::new("dumpsys")
        .arg("battery")
        .output()
        .expect("Failed to execute dumpsys command");

    // Convert output to string and print it
    let output_str = std::str::from_utf8(&output.stdout).unwrap();

    // Define the regex pattern
    // let re = Regex::new(r"(?i)charge counter:\s*(\d+).*?level:\s*(\d+).*?voltage:\s*(\d+)").unwrap();
    let re = regex::Regex::new(r"(?i)charge\s+counter:\s*(\d+)[\s\S]*?level:\s*(\d+)[\s\S]*?voltage:\s*(\d+)").unwrap();


    // Apply the regex to the log string
    if let Some(captures) = re.captures(output_str) {
        // Extract the charge counter, level, and voltage from the captures
        let charge_counter = captures.get(1).map_or("", |m| m.as_str());
        let level = captures.get(2).map_or("", |m| m.as_str());
        let voltage = captures.get(3).map_or("", |m| m.as_str());

        return Some((i64::from_str_radix(charge_counter, 10).unwrap(), i64::from_str_radix(level, 10).unwrap(), i64::from_str_radix(voltage, 10).unwrap()));
    } else {
        println!("No match found!");

        return None;
    }
}

#[cfg(target_os = "android")]
#[macro_export]
macro_rules! start_stat_tracking {
    () => {{
        let battery_before = chorus::get_battery_info_via_dumpsys().expect("failed to get battery info");
        let peak_memory_before = chorus::get_peak_memory_usage();
        (battery_before, peak_memory_before)
    }};
}
#[cfg(target_os = "linux")]
#[macro_export]
macro_rules! start_stat_tracking {
    () => {{
        let peak_memory_before = chorus::get_peak_memory_usage();
        peak_memory_before
    }};
}
#[cfg(target_os = "macos")]
#[macro_export]
macro_rules! start_stat_tracking {
    () => {{
        0
    }};
}

#[cfg(target_os = "android")]
#[macro_export]
macro_rules! end_stat_tracking {
    ($stats:expr) => {{
        let battery_after = chorus::get_battery_info_via_dumpsys().expect("failed to get battery info");
        let peak_memory_after = chorus::get_peak_memory_usage();
        println!("Charge Counter Difference: {} mAh", ($stats.0.0 - battery_after.0));
        println!("Charge Level Difference: {} %", ($stats.0.1 - battery_after.1));
        // println!("Battery Level Difference: {} Volts", ($battery_before.2 - battery_after.2));
        println!("Peak Memory Usage Difference: {} bytes", (peak_memory_after - $stats.1));
    }};
}
#[cfg(target_os = "linux")]
#[macro_export]
macro_rules! end_stat_tracking {
    ($stats:expr) => {{
        let peak_memory_after = chorus::get_peak_memory_usage();
        println!("Peak Memory Usage Difference: {} bytes", (peak_memory_after - $stats));
    }};
}
#[cfg(target_os = "macos")]
#[macro_export]
macro_rules! end_stat_tracking {
    ($stats:expr) => {{
        0
    }};
}

pub mod utils;
pub mod crypto;
pub mod secret_recovery;
pub mod network;