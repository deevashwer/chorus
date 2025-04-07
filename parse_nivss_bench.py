import re
import matplotlib.pyplot as plt
from matplotlib.ticker import ScalarFormatter
import numpy as np

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
    time_re = re.compile(bench + r".*time:\s+\[\d+\.\d+\s([a-z]+)\s(\d+\.\d+)\s[a-z]+\s\d+\.\d+\s[a-z]+\]")
    
    match = time_re.search(log)
    if match:
        unit = match.group(1)
        avg_time = float(match.group(2))
        if unit == 'ms':
            avg_time /= 1000
            unit = 's'
        return {"avg_time": avg_time, "unit": unit}
    return None

def get_time_and_battery_info(bench, log):
    time_battery_re = re.compile(
        bench + r".*time:\s+\[\d+\.\d+\s([a-z]+)\s(\d+\.\d+)\s[a-z]+\s\d+\.\d+\s[a-z]+\]"
        r"(?:[\s\S]*?Charge Counter Difference:\s+(\d+) mAh)?",
        re.MULTILINE
    )
    samples = 10
    
    match = time_battery_re.search(log)
    if match:
        unit = match.group(1)
        avg_time = float(match.group(2))
        charge_diff = int(match.group(3))
        if unit == 'ms':
            avg_time /= 1000
            unit = 's'
        
        ret = {
            "avg_time": avg_time,
            "unit": unit,
            "charge_diff": charge_diff,
            "samples": samples,
        }
        return ret
    return None

def process_time_and_battery(stat, multiplier):
    unit = stat["unit"]
    time = stat["avg_time"] * multiplier
    charge = min((stat["charge_diff"] * multiplier) / float(stat["samples"]), total_Ah)
    return f"{time:.2f} {unit} ({charge/float(1000.0):.1f} mAh / {float(charge) * 100 / total_Ah:.1f}%)"

def process_time(stat, multiplier):
    unit = stat["unit"]
    time = stat["avg_time"] * multiplier
    return f"{time:.2f} {unit}"


def improvement(orig, imp):
    improvement_factors = [ orig / imp if imp != 0 else float('inf') for orig, imp in zip(orig, imp) ]
    return improvement_factors

def process_client_log(log_path):
    with open(log_path, 'r') as log:
        log = log.read()

    # Split the log into individual cases
    cases = log.split("case: ")[1:]  # Skip the first empty part
    
    # Define regular expressions to extract metrics
    info_re = re.compile(r'fraction:\s*(\d+),\s*committee_size:\s*(\d+),\s*threshold:\s*(\d+)')
    # charge_re = re.compile(r"Charge Counter Difference:\s+(\d+) mAh")
    dealing_size_re = re.compile(r"Dealing bytesize:\s+(\d+)")
    lite_dealing_size_re = re.compile(r"Max Lite Dealing bytesize:\s+(\d+)")

    stats = {}
    
    # Process each case
    for case_idx, case in enumerate(cases):
        info = info_re.search(case)
        # deal_time = deal_re.search(case)
        deal_time_and_battery = get_time_and_battery_info("deal", case)
        # receive_time = receive_re.search(case)
        receive_time_and_battery = get_time_and_battery_info("receive", case)
        # charge_diff = charge_re.findall(case)
        dealing_size = dealing_size_re.search(case)
        if dealing_size:
            dealing_size = int(dealing_size.group(1))
        lite_dealing_size = lite_dealing_size_re.search(case)
        if lite_dealing_size:
            lite_dealing_size = int(lite_dealing_size.group(1))
        
        fraction, committee_size, threshold = info.groups()
        fraction, committee_size, threshold = int(fraction), int(committee_size), int(threshold)
        # print(f"Fraction: {fraction}, Committee Size: {committee_size}, Threshold: {threshold}")

        multiplier = (2 * threshold) / 64
        print("receive", process_time_and_battery(receive_time_and_battery, multiplier))

        stats[case_idx] = {
            "Fraction": fraction,
            "Threshold": threshold,
            "Committee Size": committee_size,
            # "Deal Time": process_time_and_battery(deal_time_and_battery, 1),
            "Deal Time": deal_time_and_battery["avg_time"],
            # "Receive Time": process_time_and_battery(receive_time_and_battery, int(threshold)),
            "Receive Time": receive_time_and_battery["avg_time"] / float(64.0), # we did 64 parallel receives
            # "Dealing Size": convert_bytes(dealing_size),
            "Dealing Size": (dealing_size) / float(10.0 ** 9),
            # "Lite Dealing Size": convert_bytes(lite_dealing_size)
            "Lite Dealing Size": lite_dealing_size if lite_dealing_size is None else lite_dealing_size / float(10.0 ** 9)
        }
        print(stats[case_idx])
        print("---")
    return stats

def process_server_log(log_path):
    with open(log_path, 'r') as log:
        log = log.read()

    # Split the log into individual cases
    cases = log.split("case: ")[1:]  # Skip the first empty part
    
    # Define regular expressions to extract metrics
    info_re = re.compile(r'fraction:\s*(\d+),\s*committee_size:\s*(\d+),\s*threshold:\s*(\d+)')

    stats = {}
    
    # Process each case
    for case_idx, case in enumerate(cases):
        info = info_re.search(case)
        # deal_time = deal_re.search(case)
        verify_time = get_time("verify-dealing", case)
        print(verify_time)
        
        fraction, committee_size, threshold = info.groups()
        fraction, committee_size, threshold = int(fraction), int(committee_size), int(threshold)

        stats[case_idx] = {
            "Fraction": fraction,
            "Threshold": threshold,
            "Committee Size": committee_size,
            # "Verify Dealing Time": process_time(verify_time, 1),
            "Verify Dealing Time": verify_time["avg_time"],
        }
        print(stats[case_idx])
        print("---")
    return stats

def plot_times(pv_nivss, sa_nivss, sa_nivss_server):
    cases = [0, 1, 2, 3]

    xticks = [pv_nivss[c]["Committee Size"] for c in cases]
    # xlabels = [f"({pv_nivss[c]['Fraction']}, {pv_nivss[c]['Committee Size']}, {sa_nivss[c]['Threshold']})" for c in cases]
    xlabels = [f"n={pv_nivss[c]['Committee Size']}\nt={sa_nivss[c]['Threshold']}\n$\kappa$={pv_nivss[c]['Fraction']}%" for c in cases]

    max_dealings_processed = [ 2 * pv_nivss[c]["Threshold"] for c in cases]
    
    # Extract data for both schemes
    pv_nivss_deal_time = [pv_nivss[c]["Deal Time"] for c in cases]
    pv_nivss_recv_time = [(pv_nivss[c]["Receive Time"] * max_dealings_processed[c]) for c in cases]
    
    sa_nivss_deal_time = [sa_nivss[c]["Deal Time"] for c in cases]
    sa_nivss_recv_time = [(sa_nivss[c]["Receive Time"] * sa_nivss[c]["Threshold"]) for c in cases]

    sa_nivss_verify_time = [sa_nivss_server[c]["Verify Dealing Time"] for c in cases]
    
    fig, ax1 = plt.subplots(figsize=(3,3))

    print([
        orig / imp if imp != 0 else float('inf')  # Avoid division by zero
        for orig, imp in zip(pv_nivss_deal_time, sa_nivss_deal_time)
    ])
    print("deal_time_improvement", improvement(pv_nivss_deal_time, sa_nivss_deal_time))
    print("recv_time_improvement", improvement(pv_nivss_recv_time, sa_nivss_recv_time))
    print("sa_nivss_server_time", sa_nivss_verify_time)
    print("pv_nivss_recv_time", pv_nivss_recv_time)
    print("pv_nivss_deal_time", pv_nivss_deal_time)
    
    # Plot dealing times
    # ax1.set_xlabel('(f, n, t)')
    # ax1.set_ylabel('Time (s)')
    ax1.plot(xticks, pv_nivss_deal_time, marker='o', linestyle='-', color='tab:red', label="cgVSS Dealer")
    ax1.plot(xticks, pv_nivss_recv_time, marker='x', linestyle='-', color='tab:red', label="cgVSS Recipient")
    ax1.plot(xticks, sa_nivss_deal_time, marker='o', linestyle='--', color='tab:blue', label="saNIVSS Dealer")
    ax1.plot(xticks, sa_nivss_recv_time, marker='x', linestyle='--', color='tab:blue', label="saNIVSS Recipient")
    ax1.plot(xticks, sa_nivss_verify_time, marker='^', linestyle='--', color='tab:blue', label="saNIVSS Server")
    # ax1.tick_params(axis='y', labelcolor='tab:blue')

    ax1.set_yscale('log')
    ax1.set_xscale('log', base=2)
    ax1.set_xticks(xticks)
    ax1.set_xticklabels(xlabels)
    # Add dotted gray grid lines for y-axis
    ax1.grid(which='major', axis='y', color='gray', linestyle=':', linewidth=0.5)
    
    # Add legend
    fig.tight_layout()
    # fig.subplots_adjust(top=1, bottom=0, left=0, right=1)
    # fig.legend(loc="upper left", bbox_to_anchor=(0.00, 1), bbox_transform=ax1.transAxes)
    # plt.title("saNIVSS vs cgVSS Runtime")
    # plt.show()
    plt.savefig("nivss-time.pdf", bbox_inches='tight')

def plot_comms(pv_nivss, sa_nivss):
    cases = [0, 1, 2, 3]

    xticks = [pv_nivss[c]["Committee Size"] for c in cases]
    xlabels = [f"n={pv_nivss[c]['Committee Size']}\nt={sa_nivss[c]['Threshold']}\n$\kappa$={pv_nivss[c]['Fraction']}%" for c in cases]

    max_dealings_recvd = [ pv_nivss[c]["Committee Size"] for c in cases]
    
    # Extract data for both schemes
    pv_nivss_deal_comm = [pv_nivss[c]["Dealing Size"] for c in cases]
    pv_nivss_recv_comm = [(pv_nivss[c]["Dealing Size"] * max_dealings_recvd[c]) for c in cases]

    # only send to threshold committee members
    pv_nivss_server_comm = [(pv_nivss[c]["Dealing Size"] * max_dealings_recvd[c] + pv_nivss[c]["Dealing Size"] * (max_dealings_recvd[c] * max_dealings_recvd[c])) for c in cases]
    
    sa_nivss_deal_comm = [sa_nivss[c]["Dealing Size"] for c in cases]
    sa_nivss_recv_comm = [(sa_nivss[c]["Lite Dealing Size"] * sa_nivss[c]["Threshold"]) for c in cases]

    # only send to threshold committee members
    sa_nivss_server_comm = [(sa_nivss[c]["Dealing Size"] * max_dealings_recvd[c] + sa_nivss[c]["Lite Dealing Size"] * (sa_nivss[c]["Threshold"] * max_dealings_recvd[c])) for c in cases]

    print("deal_comm_improvement", improvement(pv_nivss_deal_comm, sa_nivss_deal_comm))
    print("recv_comm_improvement", improvement(pv_nivss_recv_comm, sa_nivss_recv_comm))
    print("server_comm_improvement", improvement(pv_nivss_server_comm, sa_nivss_server_comm))
    print("pv_nivss_recv_comm", pv_nivss_recv_comm)
    print("pv_nivss_server_comm", pv_nivss_server_comm)
    print("pv_nivss_deal_comm", pv_nivss_deal_comm)
    
    fig, ax1 = plt.subplots(figsize=(3,3))
    
    # add minorticks manually
    y_vals = [10**i for i in range(-3, 4 + 1)]
    for y in y_vals:
        ax1.axhline(y=y, color='gray', linestyle=':', linewidth=0.5, alpha=1)
    
    # ax1.set_xlabel('(f, n, t)')
    # ax1.set_ylabel('Communication (GiB)')
    ax1.plot(xticks, pv_nivss_deal_comm, marker='o', linestyle='-', color='tab:red', label="cgVSS Dealer")
    ax1.plot(xticks, pv_nivss_recv_comm, marker='x', linestyle='-', color='tab:red', label="cgVSS Recipient")
    ax1.plot(xticks, pv_nivss_server_comm, marker='^', linestyle='-', color='tab:red', label="cgVSS Server")
    ax1.plot(xticks, sa_nivss_deal_comm, marker='o', linestyle='--', color='tab:blue', label="saNIVSS Dealer")
    ax1.plot(xticks, sa_nivss_recv_comm, marker='x', linestyle='--', color='tab:blue', label="saNIVSS Recipient")
    ax1.plot(xticks, sa_nivss_server_comm, marker='^', linestyle='--', color='tab:blue', label="saNIVSS Server")

    ax1.set_yscale('log')
    ax1.set_xscale('log', base=2)
    ax1.set_xticks(xticks)
    ax1.set_xticklabels(xlabels)
    ax1.minorticks_on()
    # Add dotted gray grid lines for y-axis
    ax1.grid(which='major', axis='y', color='gray', linestyle=':', linewidth=0.5)
    
    # Add legend
    fig.tight_layout()
    # fig.subplots_adjust(top=1, bottom=0, left=0, right=1)
    # fig.legend(loc="upper left", bbox_to_anchor=(0.00, 1), bbox_transform=ax1.transAxes)
    # plt.title("saNIVSS vs cgVSS Communication")
    # plt.show()
    plt.savefig("nivss-comm.pdf", bbox_inches='tight')

def plot_legend():
    fig, ax = plt.subplots(figsize=(6, 0.5))  # Adjust width for more legend entries
    handles = [plt.Line2D([0], [0], marker='o', linestyle='-', color='tab:red', label='cgVSS Share'),
               plt.Line2D([0], [0], marker='o', linestyle='--', color='tab:blue', label='saVSS Share'),
               plt.Line2D([0], [0], marker='x', linestyle='-', color='tab:red', label='cgVSS Reconst.'),
               plt.Line2D([0], [0], marker='x', linestyle='--', color='tab:blue', label='saVSS Reconst.'),
               plt.Line2D([0], [0], marker='^', linestyle='-', color='tab:red', label='cgVSS Server'),
               plt.Line2D([0], [0], marker='^', linestyle='--', color='tab:blue', label='saVSS Server')]
    ax.legend(handles=handles, loc='center', ncol=3, frameon=True)  # Horizontal layout
    ax.axis('off')  # Hide axis
    # plt.show()
    plt.savefig("nivss-legend.pdf", bbox_inches='tight')

pv_nivss_client_stats = process_client_log("pv_nivss_client_parallel.log")
sa_nivss_client_stats = process_client_log("sa_nivss_client_parallel.log")
sa_nivss_server_stats = process_server_log("sa_nivss_server.log")
# Example usage
plot_times(pv_nivss_client_stats, sa_nivss_client_stats, sa_nivss_server_stats)
plot_comms(pv_nivss_client_stats, sa_nivss_client_stats)
plot_legend()