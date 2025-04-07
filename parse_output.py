import re
import pandas as pd
import os
import copy
from tabulate import tabulate

# Regex patterns to extract relevant information
time_pattern = re.compile(r'([\w-]+(?: #\d+)?)\s+time:\s+\[\s*[\d\.]+\s*([a-z]+)\s+([\d\.]+\s*[a-z]+)\s+[\d\.]+\s*[a-z]+\s*\]')
bytes_pattern = re.compile(r'(.+)\s+bytesize:\s+(\d+)\s+bytes')
info_pattern = re.compile(r'corrupt fraction:\s*(\d+),\s*threshold:\s*(\d+),\s*committee_size:\s*(\d+)')

with open('output.log', 'r') as file:
    output = file.read()

# Data extraction
time_data = time_pattern.findall(output)
bytes_data = bytes_pattern.findall(output)
info_data = info_pattern.findall(output)

# Function to convert bytes to a human-readable format
def convert_bytes(size):
    size = float(size)
    for unit in ['bytes', 'KB', 'MB', 'GB', 'TB']:
        if size < 1024:
            return f"{size:.2f} {unit}"
        size /= 1024

# Create lists for DataFrame
op_runtime = []
runtime = []
op_bytesize = []
bytesize = []
corruption_info = {}

# Process the time data
for entry in time_data:
    op, time1, time2 = entry
    op_runtime.append(op)
    runtime.append(time2)

# Process the bytes data
for entry in bytes_data:
    op, size = entry
    size = convert_bytes(size)
    op_bytesize.append(op)
    bytesize.append(size)

# Process the corruption info
for entry in info_data:
    fraction, threshold, committee_size = entry
    corruption_info[fraction] = {'Threshold': threshold, 'Committee Size': committee_size}

stages = {
    'Committee-Selection': {
        'C': ('runtime', 'sortition'),
        'S': ('runtime', 'process-committee'),
        'C->S': ('bytesize', 'Nomination'),
        'S->C': ('bytesize', 'Committee'),
    },
    'DKG-step-1': {
        'C': ('runtime', 'dkg-contribute'),
        'S': ('runtime', 'process-state'),
        'C->S': ('bytesize', 'Contribution'),
        'S->C': ('bytesize', 'DKGState'),
    },
    'DKG-step-2': {
        'C': ('runtime', 'handover-state'),
        'S': ('runtime', 'process-state'),
        'C->S': ('bytesize', 'Handover (DKG)'),
        'S->C': ('bytesize', 'Handover (DKG)'),
    },
    'Handover (Typical)': {
        'C': ('runtime', 'handover-state'),
        'S': ('runtime', 'process-state'),
        'C->S': ('bytesize', 'Handover (Typical)'),
        'S->C': ('bytesize', 'Handover (Typical)'),
    },
}

runtime_idx = 0
bytesize_idx = 0

corruption_fractions = [5, 10, 15, 20]
stages_numbers = {}
for fraction in corruption_fractions:
    stages_numbers[fraction] = copy.deepcopy(stages)
    for stage in stages.keys():
        for metric in stages[stage].keys():
            if stages[stage][metric][0] == 'runtime':
                while stages[stage][metric][1] not in op_runtime[runtime_idx]:
                    runtime_idx += 1
                stages_numbers[fraction][stage][metric] = runtime[runtime_idx]
                print(f"{fraction} {stage} {metric} {runtime[runtime_idx]}")
                runtime_idx += 1
            else:
                while stages[stage][metric][1] not in op_bytesize[bytesize_idx]:
                    bytesize_idx += 1
                stages_numbers[fraction][stage][metric] = bytesize[bytesize_idx]
                print(f"{fraction} {stage} {metric} {bytesize[bytesize_idx]}")
                bytesize_idx += 1

# Prepare the table data
rows = []
for fraction, stages in stages_numbers.items():
    for metric in stages[next(iter(stages))]:  # Iterate over metrics
        row = {'Fraction': f"{fraction}% (n={corruption_info[str(fraction)]['Committee Size']},t={corruption_info[str(fraction)]['Threshold']})", 'Metric': metric}
        for stage, metrics in stages.items():
            row[stage] = metrics[metric]
        rows.append(row)
    # Add an empty row after each fraction for visual separation
    row = {'Fraction': '-' * len('Fraction'), 'Metric': '-' * len('Metric')}
    for stage in stages.keys():
        row[stage] = '-' * len(stage)
    rows.append(row)

# Create a DataFrame
df = pd.DataFrame(rows)

# Display the table
# print(df.to_string(index=False))
print(tabulate(df, headers='keys', tablefmt='grid', showindex=False))

print(info_data)
