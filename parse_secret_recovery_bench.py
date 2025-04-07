import re
import matplotlib.pyplot as plt
from matplotlib.ticker import ScalarFormatter
from pprint import pprint
from typing import List, Dict, Union
import math
import numpy as np
from collections import defaultdict

total_Ah = 3880000

# Function to convert bytes to a human-readable format
def convert_bytes(size):
    if size is None:
        return None
    size = float(size)
    for unit in ['bytes', 'KB', 'MB', 'GB', 'TB']:
        if size < 1000:
            return f"{size:.2f} {unit}"
        size /= 1000

def get_time(bench, log):
    time_re = re.compile(bench + r"(?:[\s\S]*?)time:\s+\[\d+\.\d+\s([a-z]+)\s(\d+\.\d+)\s[a-z]+\s\d+\.\d+\s[a-z]+\]")
    
    '''
    match = time_re.search(log)
    if match:
        unit = match.group(1)
        avg_time = float(match.group(2))
        print(f"{bench}: {avg_time:.3f} {unit}")
        if unit == "ms":
            unit = "s"
            avg_time /= 1000.0
        return {"avg_time": avg_time, "unit": unit}
    '''
    matches = time_re.findall(log)
    extracted_times = []

    for match in matches:
        unit = match[0]
        avg_time = float(match[1])
        print(f"{bench}: {avg_time:.3f} {unit}")
        if unit == "ms":
            unit = "s"
            avg_time /= 1000.0
        extracted_times.append({"avg_time": avg_time, "unit": unit})
    
    return extracted_times if extracted_times else None
    return None

def get_time_and_battery_info(bench, log):
    # time_battery_re = re.compile(
    #     bench + r".*time:\s+\[\d+\.\d+\s([a-z]+)\s(\d+\.\d+)\s[a-z]+\s\d+\.\d+\s[a-z]+\]"
    #     r"(?:[\s\S]*?Charge Counter Difference:\s+(\d+) mAh)?",
    #     re.MULTILINE
    # )
    time_battery_re = re.compile(
        re.escape(bench) + r"(?:[\s\S]*?)time:\s+\[\d+\.\d+\s([a-z]+)\s(\d+\.\d+)\s[a-z]+\s\d+\.\d+\s[a-z]+\]"
        r"(?:[\s\S]*?Charge Counter Difference:\s+(\d+) mAh)?",
        re.MULTILINE | re.DOTALL
    )
    iterations_re = re.compile(
        bench + r": Collecting.*?\(\s*(\d+)\s*iterations\)",
        re.MULTILINE | re.DOTALL
    )
    samples = 10
    match = iterations_re.search(log)
    if match:
        samples = int(match.group(1))
    
    match = time_battery_re.search(log)
    if match:
        unit = match.group(1)
        avg_time = float(match.group(2))
        charge_diff = float(match.group(3))/float(samples) if match.group(3) else 0.0
        print(f"{bench}: {avg_time:.3f} {unit} ({charge_diff/float(1000.0):.3f} mAh / {float(charge_diff) * 100 / total_Ah:.3f}%)")

        if unit == "ms":
            unit = "s"
            avg_time /= 1000.0
        
        return {
            "avg_time": avg_time,
            "unit": unit,
            "charge_diff": charge_diff,
        }
    return None

def process_time_and_battery(stat, multiplier):
    unit = stat["unit"]
    time = stat["avg_time"] * multiplier
    if unit == "s" and time < 1:
        time *= 1000
        unit = "ms"
    charge = min((stat["charge_diff"] * multiplier), total_Ah)
    return (f"{time:.3f} {unit}", f"{charge/float(1000.0):.3f} mAh / {float(charge) * 100 / total_Ah:.3f}%")

def process_time(stat, multiplier):
    unit = stat["unit"]
    time = stat["avg_time"] * multiplier
    if unit == "s" and time < 1:
        time *= 1000
        unit = "ms"
    return f"{time:.3f} {unit}"

def process_bytesize(stat, log, metric_name, dict_name):
    metric_name_re = re.compile(re.escape(metric_name) + r" bytesize:\s+(\d+)")
    metric_size = metric_name_re.search(log)
    if metric_size:
        metric_size = int(metric_size.group(1))
    print(metric_name, convert_bytes(metric_size))
    stat[dict_name] = metric_size

def add_dicts(dicts: List[Dict[str, Union[float, int, str]]]) -> Dict[str, Union[float, int, str]]:
    result = {
        "avg_time": 0.0,
        "unit": dicts[0]["unit"],  # assumes all units are the same
        "charge_diff": 0,
    }

    for d in dicts:
        result["avg_time"] += d["avg_time"]
        if "charge_diff" in d:
            result["charge_diff"] += d["charge_diff"]

        # Check for consistent units
        if result["unit"] != d["unit"]:
            raise ValueError("Inconsistent units found in the dictionaries.")

    return result

def readable_time(minutes: int) -> str:
    months = minutes // (30 * 24 * 60)  # 30 days per month
    days = (minutes % (30 * 24 * 60)) // (24 * 60)
    hours = (minutes % (24 * 60)) // 60
    remaining_minutes = minutes % 60

    parts = []
    if months > 0:
        parts.append(f"{months} month{'s' if months > 1 else ''}")
    if days > 0:
        parts.append(f"{days} day{'s' if days > 1 else ''}")
    if hours > 0:
        parts.append(f"{hours} hour{'s' if hours > 1 else ''}")
    if remaining_minutes > 0:
        parts.append(f"{remaining_minutes} minute{'s' if remaining_minutes > 1 else ''}")

    return " ".join(parts) if parts else "0 minutes"

def process_client_log(log_path):
    with open(log_path, 'r') as log:
        log = log.read()

    runs = log.split("case: ")[1:]  # Skip the first empty part

    info_re = re.compile(r'\s*(\d+),\s*corrupt fraction:\s*(\d+),\s*fail fraction:\s*(\d+),\s*threshold:\s*(\d+),\s*committee_size:\s*(\d+),\s*num_clients:\s*(\d+)')

    stats = {}
    
    for run in runs: 
        # Process each case
        info = info_re.search(run)
        client_new_time = get_time("client-new", run)
        sortition_time_and_battery = get_time_and_battery_info("sortition", run)
        # with network
        # dkg_contribute_time_and_battery = get_time_and_battery_info("dkg-contribute-with-network", run)
        # dkg_handover_time_and_battery = get_time_and_battery_info("handover-dkg-with-network", run)
        # typical_handover_time_and_battery = get_time_and_battery_info("handover-typical-with-network", run)
        # without network
        dkg_contribute_time_and_battery = get_time_and_battery_info("dkg-contribute", run)
        dkg_handover_time_and_battery = get_time_and_battery_info("handover-dkg", run)
        typical_handover_time_and_battery = get_time_and_battery_info("handover-typical", run)
        backup_time_and_battery = get_time_and_battery_info("backup", run)
        recovery_request_time_and_battery = get_time_and_battery_info("recovery-request", run)
        # TODO: fix this
        recover_time_and_battery = {'avg_time': 0.0, 'unit': 's', 'charge_diff': 0.0} #get_time_and_battery_info("recover", run)

        expected_requests_re = re.compile(r"expected_requests_per_epoch:\s+(\d+)")
        match = expected_requests_re.search(run)
        if match:
            expected_requests = int(match.group(1))
        
        case, corrupt_fraction, fail_fraction, threshold, committee_size, num_clients = info.groups()
        corrupt_fraction, fail_fraction, committee_size, threshold, num_clients = int(corrupt_fraction), int(fail_fraction), int(committee_size), int(threshold), int(num_clients)

        stat = {
            "Corrupt Fraction": corrupt_fraction,
            "Fail Fraction": fail_fraction,
            "Threshold": threshold,
            "Committee Size": committee_size,
            "Num Clients": num_clients,
            "Client New Time": client_new_time,
            "Sortition Time": sortition_time_and_battery,
            "DKG Contribute Time": dkg_contribute_time_and_battery,
            "DKG Handover Time": dkg_handover_time_and_battery,
            "Typical Handover Time": typical_handover_time_and_battery,
            "Backup Time": backup_time_and_battery,
            "Recovery Request Time": recovery_request_time_and_battery,
            "Recover Time": recover_time_and_battery,
            "Expected Requests": expected_requests,
        }

        # Regular expression to capture task name and runtime with units
        pattern = r"End:\s+(.+?)\s+\.+([0-9.]+)(s|ms|µs|ns)"

        # Dictionary to store total times and counts for each task
        time_data = defaultdict(lambda: {"total": 0, "count": 0})

        # Conversion factors to seconds
        conversion = {
            "s": 1,
            "ms": 1e-3,
            "µs": 1e-6,
            "ns": 1e-9,
        }

        # Extract task name and time values
        matches = re.findall(pattern, run)

        for task, value, unit in matches:
            # Convert runtime to seconds
            time_in_seconds = float(value) * conversion.get(unit, 1)
            # Aggregate total time and count occurrences
            time_data[task]["total"] += time_in_seconds
            time_data[task]["count"] += 1

        keys = ["download for dkg-contribute", \
                "upload for dkg-contribute", \
                "download for handover-dkg", \
                "upload for handover-dkg", \
                "download for handover-typical", \
                "upload for handover-typical", \
                "verify next committee", \
                "verify prev committee", \
                "verify consistency between states", \
                "receive shares", \
                "verify recovery request nizks", \
                "verify merkle proof", \
                "compute recovery response", \
                "reshare", \
               ]
        # Calculate and display averages
        for key in keys:
            if time_data[key]["count"] == 0:
                time_data[key]["avg"] = 0.0
                print(f"No time data found in the log for {key}. Setting to 0.")
            else:
                time_data[key]["avg"] = time_data[key]["total"] / time_data[key]["count"]
                print(f"{key}: {time_data[key]['avg']:.6f} seconds")
        stat["Committee_Selection Time"] = time_data["verify next committee"]["avg"] + time_data["verify prev committee"]["avg"]
        stat["Share Time"] = time_data["reshare"]["avg"]
        stat["Reconstruct Time"] = time_data["verify consistency between states"]["avg"] + time_data["receive shares"]["avg"]
        stat["Process Recovery Request Time"] = time_data["verify recovery request nizks"]["avg"] + time_data["verify merkle proof"]["avg"] + time_data["compute recovery response"]["avg"]
        stat["DKG Contribute Comm Time"] = time_data["download for dkg-contribute"]["avg"] + time_data["upload for dkg-contribute"]["avg"]
        stat["DKG Handover Comm Time"] = time_data["download for handover-dkg"]["avg"] + time_data["upload for handover-dkg"]["avg"]
        stat["Typical Handover Comm Time"] = time_data["download for handover-typical"]["avg"] + time_data["upload for handover-typical"]["avg"]

        stats[(case, num_clients)] = stat
    # pprint(stats)
    return stats

def process_server_log(log_path):
    with open(log_path, 'r') as log:
        log = log.read()

    # Define regular expressions to extract metrics

    runs = log.split("case: ")[1:]  # Skip the first empty part

    info_re = re.compile(r'\s*(\d+),\s*corrupt fraction:\s*(\d+),\s*fail fraction:\s*(\d+),\s*threshold:\s*(\d+),\s*committee_size:\s*(\d+),\s*num_clients:\s*(\d+)')

    stats = {}

    for run in runs: 
        stat = {}

        # Process each case
        info = info_re.search(run)
        process_committee_times = get_time("process-committee", run)
        process_state_times = get_time("process-state", run)
        process_recovery_request_time = get_time("server-process-recovery-requests", run)
        process_recovery_response_time = get_time("server-process-recovery-responses", run)
        stat["Process Committee Time"] = process_committee_times[0]
        stat["DKG-step-1 Time"] = process_state_times[0]
        stat["DKG-step-2 Time"] = process_state_times[1]
        stat["Handover Time"] = process_state_times[2]
        stat["Process Recovery Request Time"] = process_recovery_request_time[0]
        stat["Process Recovery Response Time"] = process_recovery_response_time[0]

        process_bytesize(stat, run, "Max Nomination", "C->S Committee Selection")
        process_bytesize(stat, run, "Max Contribution", "C->S DKG-phase-1")
        process_bytesize(stat, run, "Max Handover (DKG)", "C->S DKG-phase-2")
        process_bytesize(stat, run, "Max Handover (Typical)", "C->S Handover")
        process_bytesize(stat, run, "Max Recovery Response Batch", "C->S Recovery Response Batch")
        process_bytesize(stat, run, "Max Recovery Responses", "S->C Recovery Response")
        process_bytesize(stat, run, "Backup Ciphertext", "Backup Ciphertext")
        process_bytesize(stat, run, "Recovery Request", "Recovery Request")
        process_bytesize(stat, run, "Recovery Request Batch", "Recovery Request Batch")
        process_bytesize(stat, run, "committee_0", "Committee")
        process_bytesize(stat, run, "public_state_epoch_2", "Public State")
        process_bytesize(stat, run, "Max commstate_1", "Max Commstate_1")
        process_bytesize(stat, run, "Max commstate_2", "Max Commstate_2")
        process_bytesize(stat, run, "Max commstate_3", "Max Commstate_3")
        process_bytesize(stat, run, "prev_state", "Prev State Coeffs")

        expected_requests_re = re.compile(r"expected_requests_per_epoch:\s+(\d+)")
        match = expected_requests_re.search(run)
        if match:
            stat["Expected Requests"] = int(match.group(1))
        
        case, corrupt_fraction, fail_fraction, threshold, committee_size, num_clients = info.groups()
        corrupt_fraction, fail_fraction, committee_size, threshold, num_clients = int(corrupt_fraction), int(fail_fraction), int(committee_size), int(threshold), int(num_clients)
        stat["Corrupt Fraction"] = corrupt_fraction
        stat["Fail Fraction"] = fail_fraction
        stat["Threshold"] = threshold
        stat["Committee Size"] = committee_size
        stat["Num Clients"] = num_clients

        stats[(case, num_clients)] = stat

    # pprint(stats)
    return stats

def sr_client_table(client_stats, server_stats):
    key = next(iter(client_stats))
    client_stats_ = client_stats[key]
    server_stats_ = server_stats[key]
    backup = {
        "Time": process_time_and_battery(client_stats_["Backup Time"], 1)[0],
        "Communication": convert_bytes(server_stats_["Backup Ciphertext"]),
        "Battery": process_time_and_battery(client_stats_["Backup Time"], 1)[1],
        "Memory": "N/A",
    }
    groth_pk_size = 205090704
    recovery_time = add_dicts([client_stats_["Recover Time"], client_stats_["Recovery Request Time"]])
    recovery = {
        "Time": process_time_and_battery(recovery_time, 1)[0],
        "Communication": convert_bytes(server_stats_["Recovery Request"] + server_stats_["S->C Recovery Response"] + groth_pk_size),
        "Battery": process_time_and_battery(recovery_time, 1)[1],
        "Memory": "N/A",
    }
    print("Recovery without public params", convert_bytes(server_stats_["Recovery Request"] + server_stats_["S->C Recovery Response"]))
    print("Backup", backup)
    print("Recovery", recovery)
    pass

def on_committee_frequency(client_stats):
    Ah_for_10K_requests = 38800
    Ah_for_1_request = Ah_for_10K_requests / 10000
    battery_per_request = (Ah_for_1_request / total_Ah) * 100

    for case, num_clients in client_stats:
        minute_multiplier = 2
        probability_of_sortition = client_stats[(case, num_clients)]["Committee Size"] / float(num_clients)
        expected_epochs_before_sortition = math.ceil(1 / probability_of_sortition)
        expected_time_before_sortition = readable_time(minute_multiplier * expected_epochs_before_sortition)

        km_client_time = client_stats[(case, num_clients)]["Typical Handover Time"]
        recoveries_per_epoch = client_stats[(case, num_clients)]["Expected Requests"]
        Ah_for_requests = recoveries_per_epoch * Ah_for_1_request
        battery_for_requests = recoveries_per_epoch * battery_per_request

        print(f"case: {case}, N: {num_clients}, recoveries per epoch: {recoveries_per_epoch}, expected time before sortition: {expected_time_before_sortition}, km_client_time: {process_time_and_battery(km_client_time, 1)}, requests cost: {Ah_for_requests}Ah/{battery_for_requests}%")

def one_time_costs(client_stats, server_stats):
    for key in client_stats:
        assert key in server_stats
        client_stat = client_stats[key]
        server_stat = server_stats[key]
        n = client_stat["Committee Size"]
        t = client_stat["Threshold"]
        # two committee selections, process_state
        client_time_1 = add_dicts([client_stat["Sortition Time"]] * 2 + [client_stat["DKG Contribute Time"]])
        client_comm_1 = 2 * server_stat["C->S Committee Selection"] + server_stat["C->S DKG-phase-1"] + 2 * server_stat["Committee"]
        server_time_1 = add_dicts([server_stat["Process Committee Time"]] * 2 + [server_stat["DKG-step-1 Time"]])
        server_comm_1 = n * client_comm_1
        # one commitee selection, process_state #2
        client_time_2 = add_dicts([client_stat["Sortition Time"]] + [client_stat["DKG Handover Time"]])
        client_comm_2 = server_stat["Max Commstate_1"] + server_stat["C->S Committee Selection"] + server_stat["C->S DKG-phase-2"] + 3 * server_stat["Committee"]
        server_time_2 = add_dicts([server_stat["Process Committee Time"], server_stat["DKG-step-2 Time"]])
        server_comm_2 = n * client_comm_2

        server = {
            "Time Epoch 1": process_time(server_time_1, 1),
            "Communication Epoch 1": convert_bytes(server_comm_1),
            "Time Epoch 2": process_time(server_time_2, 1),
            "Communication Epoch 2": convert_bytes(server_comm_2),
        }

        client = {
            "Time Epoch 1": process_time_and_battery(client_time_1, 1)[0],
            "Communication Epoch 1": convert_bytes(client_comm_1),
            "Time Epoch 2": process_time_and_battery(client_time_2, 1)[0],
            "Communication Epoch 2": convert_bytes(client_comm_2),
        }

        print(f"Case: {key[0]}, N: {key[1]}, Server: {server}, Client: {client}")

def server_costs(server_stats):
    # process handover, process request, process response
    for (case, num_clients) in server_stats:
        stat = server_stats[(case, num_clients)]
        committee_selection_time = stat["Process Committee Time"]["avg_time"]
        handover_time = stat["Handover Time"]["avg_time"]
        process_request_time = stat["Process Recovery Request Time"]["avg_time"]
        process_response_time = stat["Process Recovery Response Time"]["avg_time"]

        n = stat["Committee Size"]
        t = stat["Threshold"]

        committee_selection_comm = n * stat["C->S Committee Selection"]
        # handover comm = new_public_state, prev_committee, curr_committee, next_committee, prev_state, curr_state, reqs,
        handover_comm = n * (stat["C->S Handover"] + stat['Max Commstate_2'] + 3 * stat['Committee'] + stat["Prev State Coeffs"] + stat["Public State"])
        handover_comm_egress = handover_comm - n * stat["C->S Handover"]
        request_comm =  n * stat["Recovery Request Batch"] + (stat["Expected Requests"] * stat["Recovery Request"])
        request_comm_egress = n * stat["Recovery Request Batch"]
        response_comm = n * stat["C->S Recovery Response Batch"] + (stat["Expected Requests"] * stat["S->C Recovery Response"])
        response_comm_egress = stat["Expected Requests"] * stat["S->C Recovery Response"]

        server_costs = {
            "ECPSS Time": handover_time + committee_selection_time,
            "Recovery Time": process_request_time + process_response_time,
            "ECPSS Comm": convert_bytes(handover_comm + committee_selection_comm),
            "Recovery Comm": convert_bytes(request_comm + response_comm),
        }

        request_multiplier = 5
        compute_cost_per_month = 1930.52 # (us-central-1)
        minute_multiplier = 2
        epochs_per_month = (60 * 24 * 30) / minute_multiplier
        network_use_per_month = ((handover_comm_egress + request_multiplier * (request_comm_egress + response_comm_egress)) / float(10.0 ** 9)) * epochs_per_month
        cost_per_gb = 0.085 # what Signal pays for network
        network_cost_per_month = network_use_per_month * cost_per_gb
        storage_per_client = 204
        storage_cost_per_month = 0.023
        storage_cost = (storage_per_client * num_clients / float(10.0 ** 9)) * storage_cost_per_month
        server_costs["Storage Cost"] = storage_cost
        server_costs["Compute Cost"] = compute_cost_per_month
        server_costs["Network Cost"] = network_cost_per_month
        server_costs["Total Cost"] = compute_cost_per_month + network_cost_per_month + storage_cost
        print(f"Costs for Case={case}, N={num_clients}: {server_costs}")

def client_breakdown(client_stats, server_stats):
    client_breakdown = {}
    for (case, num_clients) in client_stats:
        assert (case, num_clients) in server_stats
        default_client_stat = client_stats[(case, 10**6)]
        client_stat = client_stats[(case, num_clients)]
        server_stat = server_stats[(case, num_clients)]
        stat = {}
        stat["Retrieve Committees"] = {
            "Time": default_client_stat["Committee_Selection Time"],
            "Comm": (3 * server_stat["Committee"]) / float(10.0 ** 6),
        }
        stat["Handover"] = {
            "Time": default_client_stat["Share Time"] + default_client_stat["Reconstruct Time"],
            "Comm": (server_stat["C->S Handover"] + server_stat["Max Commstate_2"] + server_stat["Prev State Coeffs"]) / float(10.0 ** 6),
        }
        stat["Network"] = {
            "Time": client_stat["Typical Handover Comm Time"],
            "Comm": 0.0,
        }
        stat["Process Requests"] = {
            "Time": client_stat["Process Recovery Request Time"],
            "Comm": (server_stat["Recovery Request Batch"] + server_stat["C->S Recovery Response Batch"]) / float(10.0 ** 6),
        }
        client_breakdown[num_clients] = stat
    print(client_breakdown)

    N_list = list(client_breakdown.keys())
    # Data for the stacked bars
    categories = list(client_breakdown[N_list[0]].keys())
    bar_labels = ['$N=10^6$', '$N=10^7$', '$N=10^8$']
    for metric in ["Time", "Comm"]:
        data = []
        for key in categories:
            data.append([client_breakdown[N][key][metric] for N in N_list])

        for N in N_list:
            total_cost = sum([client_breakdown[N][key][metric] for key in categories])
            print(f"Total {metric} for N={N}: {total_cost}")

        # Colors for each category
        colors = ['skyblue', 'salmon', 'lightgreen', 'gold']
        # colors = ['skyblue', 'salmon', 'lightgreen']

        # Plotting
        fig, ax = plt.subplots(figsize=(2.5,2.5))

        positions = np.arange(len(bar_labels)) * 0.2  # Positions for the bars
        # ax.grid(which='major', axis='y', color='gray', linestyle=':', linewidth=0.5)

        # Stacking bars with outlines
        bottom = np.zeros(len(bar_labels))  # Initialize the bottom for stacking
        for i, (cat, color) in enumerate(zip(categories, colors)):
            ax.bar(positions, data[i], bottom=bottom, label=cat, color=color, edgecolor='black', linewidth=1.0, width=0.1)
            # ax.bar(positions, data[i], bottom=bottom, label=cat, color=color, width=0.1)
            bottom += data[i]  # Update bottom to add the next stack level

        ax.set_xticks(positions)
        ax.set_xticklabels(bar_labels)
        max_height = max(bottom)  # Get the maximum bar height
        ax.set_ylim(0, max_height * 1.05)

        # plt.show()
        plt.tight_layout()
        plt.savefig(f"client_breakdown_{metric}.pdf", bbox_inches='tight')

    # Create the legend
    fig, ax = plt.subplots(figsize=(6, 0.5))
    for i, (cat, color) in enumerate(zip(categories, colors)):
        ax.bar(0, 0, color=color, label=cat)  # Position all bars at x=0
    legend = ax.legend(loc='center', ncol=2, frameon=True)  # Horizontal layout
    ax.axis('off')  # Hide axis
    plt.savefig("client-breakdown-legend.pdf", bbox_inches='tight')
    # plt.show()

client_stats = process_client_log("secret_recovery_client_case1.log")
print("========================")
server_stats = process_server_log("secret_recovery_server_case1.log")
print("========================")

print("========================")
sr_client_table(client_stats, server_stats)
print("========================")
on_committee_frequency(client_stats)
print("========================")
one_time_costs(client_stats, server_stats)
print("========================")
server_costs(server_stats)
print("========================")
client_breakdown(client_stats, server_stats)