import math
from scipy.optimize import bisect
from tqdm import tqdm

use_poisson = False  # Toggle for Poisson or Chernoff bound

compsec = 128
statsec = 64
N = 10**8

# Poisson tail log probability (Stirling Approximation) for the upper tail
def poisson_log_tail(lambda_, t):
    return (-lambda_ + t*math.log(lambda_) - t*math.log(t) + t 
            - 0.5*math.log(2*math.pi*t)) / math.log(2)

# Poisson lower tail log probability approximation using a KL-divergence style bound
def poisson_log_lower_tail(lambda_, t):
    if t <= 0:
        # P(X = 0) = exp(-lambda)
        return -lambda_ / math.log(2)
    ratio = t / lambda_
    # Using the relative entropy D(ratio || 1) = ratio*log(ratio) - ratio + 1
    return -lambda_ * (ratio * math.log(ratio) - ratio + 1) / math.log(2)

# Chernoff Bound inequality solver
def chernoff_eps(n, f, sec_param, is_privacy=True):
    a = math.log2(math.e) * (f if is_privacy else (1-f)) * n

    def inequality(eps):
        return a * eps**2 - sec_param*(2 + eps if is_privacy else 2)

    try:
        eps = bisect(lambda x: inequality(x), 1e-10, 50)
    except ValueError:
        print("inequality(1e-10)", inequality(1e-10))
        print("inequality(30)", inequality(50))
        return None

    return eps

def poisson_t(n, f, sec_param, is_privacy=True):
    if is_privacy:
        # Privacy: compute an upper bound t such that P(X >= t) <= 2^-sec_param.
        λ = n * f
        def tail_bound(t):
            return poisson_log_tail(λ, t) + sec_param #+ math.log2(N * f)
        try:
            t_sol = bisect(lambda t: tail_bound(t), λ, λ*20)
        except ValueError:
            print("tail_bound(λ)", tail_bound(λ))
            print("tail_bound(λ*20)", tail_bound(λ*20))
            return None
        return math.ceil(t_sol)
    else:
        # Availability: compute a lower bound t such that P(X <= t) <= 2^-sec_param.
        λ = n * (1-f)
        def lower_tail_bound(t):
            return poisson_log_lower_tail(λ, t) + sec_param + math.log2(λ)
        try:
            # t is expected to be below λ so we search in the interval [0, λ]
            t_sol = bisect(lambda t: lower_tail_bound(t), 0, λ)
        except ValueError:
            print("lower_tail_bound(0)", lower_tail_bound(0))
            print("lower_tail_bound(λ)", lower_tail_bound(λ))
            return None
        return math.floor(t_sol)

# Combined bound with toggle
def compute_bound(n, f, sec_param, is_privacy=True):
    if use_poisson:
        t = poisson_t(n, f, sec_param, is_privacy)
        return t, None  # eps is implicit in Poisson
    else:
        eps = chernoff_eps(n, f, sec_param, is_privacy)
        if eps is None:
            return None, None
        if is_privacy:
            t = math.ceil(n*f*(1+eps))
        else:
            t = math.ceil(n*(1-f)*(1-eps))
        return t, eps

# Example Usage
n = 250
f = 0.05
N = 10**6
t_privacy, eps_privacy = compute_bound(n, f, compsec, True)
t_availability, eps_availability = compute_bound(n, f, statsec, False)

print(f"Privacy: t={t_privacy}, eps={eps_privacy}")
print(f"Availability: t={t_availability}, eps={eps_availability}")

params = {}
def params_from_f():
    corruption_f = [0.01, 0.05, 0.10, 0.20]
    availability_f = [0.5, 0.75]
    for avail_f in availability_f:
        pbar = tqdm(total=0, position=0, leave=True)
        n = 250
        for corrupt_f in corruption_f:
            while True:
                t_2, eps_2 = compute_bound(n, avail_f, statsec, False)
                t_1, eps_1 = compute_bound(n, corrupt_f, compsec, True)
                if t_1 <= t_2 <= n:
                    print(f"Config for n={n}, corrupt_f={corrupt_f}, avail_f={avail_f}, N={N}, t={t_1}, eps={eps_1}, t upper bound={t_2}, eps={eps_2}")
                    params[(corrupt_f, avail_f)] = (n, t_1)
                    break
                n += 1
                pbar.update(1)

params_from_f()