import sympy as sp
import math
from tqdm import tqdm

def solve_quadratic_inequality(expression, variable):
    # Parse the expression and the variable
    var = sp.symbols(variable)
    
    # Solve the inequality
    solutions = sp.solve_univariate_inequality(expression, var, relational=False)
    
    return solutions

compsec = 128
statsec = 64 #80
# n_list = [250, 500, 1000, 2000]
N = 10**6

def privacy_bound(n, f, N):
    inequality = f"{math.log2(math.e) * f * n} * x**2 - {compsec} * (2 + x) >= 0"#input("Enter a quadratic inequality (e.g., x**2 - 5*x + 6 <= 0): ")
    variable = "x"
    expr = sp.sympify(inequality)
    solution = solve_quadratic_inequality(expr, variable)
    eps = None
    if isinstance(solution, sp.Union):
        for interval in solution.args:
            if interval.start > 0:
                eps = interval.start
                break
    if eps is None:
        return (None, None)
    t = math.ceil(n * f * (1 + eps))

    # print(f"The values of {variable} that satisfy the inequality {inequality} are: {eps}")
    # print(f"t = {t} (eps = {eps})")
    return (t, eps)

def availability_bound(n, f, N):
    inequality = f"{math.log2(math.e) * (1-f) * n} * x**2 - {statsec} * 2 >= 0"
    # inequality = f"{math.log2(math.e) * (1-availability_f) * n} * x**2 - {statsec} * 2 >= 0"#input("Enter a quadratic inequality (e.g., x**2 - 5*x + 6 <= 0): ")
    variable = "x"
    expr = sp.sympify(inequality)
    solution = solve_quadratic_inequality(expr, variable)
    eps = None
    if isinstance(solution, sp.Union):
        for interval in solution.args:
            if interval.start > 0:
                eps = interval.start
                break
    if eps is None:
        return (None, None)
    t = math.ceil(n * (1 - f) * (1 - eps))
    # t = math.ceil(n * (1 - availability_f) * (1 - eps))

    # print(f"The values of {variable} that satisfy the inequality {inequality} are: {eps}")
    # print(f"t = {t} (eps = {eps})")
    return (t, eps)

# def availability_bound(n, f, N):
def smallest_params_for_availability_f():
    availability_f = [0.2, 0.4, 0.6, 0.8]
    pbar = tqdm(total=0, position=0, leave=True)
    lower_bound = 104
    n = 1
    for f in availability_f:
        # finding the smallest n for this f
        while True:
            t_2, eps_2 = availability_bound(n, f, N)
            if lower_bound <= t_2 <= n:
                break
            n += 1
            pbar.update(1)
        print(f"Smallest possible (n, t) for availability_f={f}: n={n}, t={t_2}, eps={eps_2}")

def smallest_params_for_corruption_f():
    corruption_f = [0.05, 0.1, 0.15, 0.20]
    pbar = tqdm(total=0, position=0, leave=True)
    n = 1
    for f in corruption_f:
        # finding the smallest n for this f
        while True:
            t_1, eps_1 = privacy_bound(n, f, N)
            if 0 <= t_1 + 1 <= n:
                break
            n += 1
            pbar.update(1)
        print(f"Smallest possible (n, t) for corruption_f={f}: n={n}, t={t_1}, eps={eps_1}")

# for n in n_list:
# Initialize a tqdm progress bar
params = {}
def params_from_f():
    # corruption_f = [0.01, 0.05, 0.1, 0.15, 0.2]
    # availability_f = [0.25, 0.5, 0.75]
    corruption_f = [0.01, 0.05, 0.10, 0.20]
    availability_f = [0.5, 0.75]
    for avail_f in availability_f:
        pbar = tqdm(total=0, position=0, leave=True)
        n = 1
        for corrupt_f in corruption_f:
            while True:
                t_2, eps_2 = availability_bound(n, avail_f, N)
                t_1, eps_1 = privacy_bound(n, corrupt_f, N)
                if t_1 <= t_2 <= n:
                    print(f"Config for n={n}, corrupt_f={corrupt_f}, avail_f={avail_f}, N={N}, t={t_1}, eps={eps_1}, t upper bound={t_2}, eps={eps_2}")
                    params[(corrupt_f, avail_f)] = (n, t_1)
                    break
                n += 1
                pbar.update(1)
    '''
    n = 1
    for f in f_list:
        # finding the smallest n for this f
        while True:
            t_1, eps_1 = privacy_bound(n, f, N)
            t_2, eps_2 = availability_bound(n, availability_f, N)
            if t_1 + 1 <= t_2 <= n:
                break
            n += 1
            pbar.update(1)
        print(f"Smallest possible (n, t) for f={f}, N={N}: n={n}, t={t_1+1}, eps_1={eps_1}, t upper bound={t_2}, eps_2={eps_2}")
    '''

def f_from_params():
    n_list = [561, 1090, 2048, 4096, 8192]
    # pbar = tqdm(total=0, position=0, leave=True)
    availability_f = 0.5
    f = 0.01
    for n in n_list:
        # t = n / 2
        while True:
            t_1, eps_1 = privacy_bound(n, f, N)
            t_2, eps_2 = availability_bound(n, availability_f, N)
            # print(f"f={f}, n={n}, t_1={t_1}, t_2={t_2}")
            t = t_1
            if t_2 < t:
                f -= 0.01
                print(f"Largest possible f for n={n}, t={t}, N={N}: f={f:.2f}, t_1={eps_1}, t_2={eps_2}")
                break
            if t <= t_2 <= n:
                f += 0.01
                continue
            # pbar.update(1)

'''
corruption_f = 0.001#0.01
availability_f = 0.01
n=1090
t=300
while True:
    t_2, eps_2 = availability_bound(n, availability_f, N)
    t_1, eps_1 = privacy_bound(n, corruption_f, N)
    # if t_1 <= t_2 <= n:
    if not (t_1 <= t_2 <= n):
        print(f"Config for n={n}, corrupt_f={corruption_f}, avail_f={availability_f}, N={N}, t={t_1}, eps={eps_1}, t upper bound={t_2}, eps={eps_2}")
        break
    # n += 1
    availability_f += 0.01
'''
# smallest_params_for_availability_f()
# smallest_params_for_corruption_f()
params_from_f()
print(params)
# f_from_params()
