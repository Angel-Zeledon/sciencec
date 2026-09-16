# Science — Design: the scientific standard libraries

Date: 2026-09-16
Status: draft for review
Phase: F1 and later. **This note does not amend §8 of the core spec.** §8's
closed list is F0's library surface and stays closed; everything here ships as
modules alongside the toolchain, the same split `data-io.md` §2 already
established.
Builds on: `ffi-c-boundary.md` (how anything here binds to C — §1.6, §1.7 and
§5.1 already decide the BLAS question), `python-interop.md` (DLPack, and why the
array type must be a `DLTensor` field for field), `collections-and-chains.md`
(the `Iterate` vocabulary every reduction here returns into), `data-io.md`
(which already owns the `data` module).

---

## 0. What this note is

A catalogue and a set of decisions. It answers, for nine proposed modules: what
is in them, what the signatures look like in real Science, what depends on what,
what waits for F1's tensors, what is written in Science and what is linked
against code that already exists.

It answers one transversal design question that cannot be deferred without
deciding it by accident: **do physical units live in the type system?** §12
answers yes, and states precisely what F0 lacks.

### 0.1 What it is not

It is not a proposal to write these libraries now. §1 of the core spec says the
bet is verification; a language that spends its first two years reimplementing
LAPACK loses that bet by running out of time. The point of cataloguing now is to
find the *language* requirements early — §14 is the real output of this note, and
the catalogue exists to justify it.

### 0.2 `data` is already owned

The user's list includes `data`. `data-io.md` §2 already specifies it:

> Everything else — `Frame`, `Rows`, `Chunks`, and every format — lives in
> modules `data`, `data.csv`, `data.json`, `data.npy`, `data.safetensors`,
> `data.arrow`, `data.parquet`

That note is approved and thorough, and this one does not re-specify it. Two
seams are worth recording, because they are where the two notes meet:

1. `data-io.md` §11.4 explicitly defers numeric methods — "`square_root()` and
   the reductions — belong to the numerics note". **This is that note**; they are
   in `math` (§5.1) and in the reductions of §5.9.
2. `data-io.md` §11.7 leaves table operations — group-by, join, aggregate — to
   whoever designs them, and says `Frame of R` is the type they operate on.
   §7.6 here puts them in `stats.frame`, because every one of them exists to feed
   a statistical model, and because splitting them from `stats` would mean two
   modules that cannot be used apart.

---

## 1. The layering

Nothing here is a peer of everything else. The dependency order is what decides
what can be built first and what a user has to install.

```
                        ┌───────────┐
                        │   math    │   no dependencies, F0-able
                        └─────┬─────┘
             ┌────────────────┼────────────────┐
             │                │                │
       ┌─────▼─────┐    ┌─────▼─────┐   ┌──────▼──────┐
       │  linalg   │    │  physics  │   │   signal    │
       │ (BLAS/    │    │  .units   │   │  (needs FFT)│
       │  LAPACK)  │    └─────┬─────┘   └──────┬──────┘
       └─────┬─────┘          │                │
             │          ┌─────▼─────┐          │
       ┌─────▼─────┐    │   chem    │          │
       │   stats   │    └───────────┘          │
       └─────┬─────┘                           │
             │                                 │
       ┌─────▼─────┐                     ┌─────▼─────┐
       │    bio    │                     │ optimize  │
       └───────────┘                     └───────────┘
                    (optimize also needs linalg)
```

Stated as rules:

- **`math` depends on nothing** and is the only module with no external
  dependency at all. Most of it compiles in F0 today.
- **`linalg` depends on `math`** and on BLAS/LAPACK through the FFI.
- **`stats` depends on `linalg`** — regression is a least-squares solve, PCA is
  an SVD, mixed models are Cholesky. Any statistics module that avoids `linalg`
  is a module that stops at the mean.
- **`optimize` depends on `linalg`** (every quasi-Newton step is a linear solve)
  and on `math` (derivatives, line searches).
- **`physics.units` depends on `math`** and on nothing else, deliberately — it
  must be usable from `chem` and `bio` without dragging in BLAS.
- **`chem` depends on `physics.units`** — a rate constant without units is a
  number nobody can check — and on `optimize` for equilibrium solving.
- **`bio` depends on `stats`**, heavily, and barely on anything else. Sequence
  work needs no numerics; population genetics and phylogenetics are statistics.
- **`signal` depends on `math`** and on an FFT, which is the one place besides
  `linalg` where linking beats writing.

---

## 2. Written in Science, or linked against C

The FFI note's §0 settles the principle and this note applies it:

> essentially every floating point operation that matters in scientific
> computing already happens inside a C or Fortran library somebody else wrote
> and somebody else tuned. `dgemm` is thirty years of hand-written assembly per
> microarchitecture.

The rule that falls out: **link where the artefact is decades of tuning or a de
facto standard; write where the artefact is a formula.**

| Area | Route | Why |
|---|---|---|
| Dense linear algebra | **Link** BLAS + LAPACK | `ffi-c-boundary.md` §1.6–1.7 already writes the `dgemm` binding. Thirty years of per-microarchitecture assembly. Not negotiable. |
| FFT | **Link** FFTW or PocketFFT | FFTW is GPL-or-pay; PocketFFT is BSD, single file, and what SciPy ships. Default to PocketFFT vendored, FFTW `when available` (FFI §5.2). |
| Sparse direct solvers | **Link** SuiteSparse (UMFPACK, CHOLMOD) | Same argument as LAPACK, one rung less universal. |
| Special functions | **Write**, mostly | Cephes-derived algorithms are published formulas. Writing them in Science means they are generic over `F32`/`F64` and differentiable in F2 — a linked C `erf` is neither. |
| Elementary functions | **Write** over LLVM intrinsics | `sin`, `exp`, `sqrt` lower to intrinsics. No FFI. |
| Random number generation | **Write** | §1 of the core spec names "random keys that cannot be reused" as a motivating verification case. That property is unavailable if the generator is C state behind a pointer. **This one must be Science.** |
| Statistical distributions | **Write** | Formulas over special functions. |
| Optimisation algorithms | **Write** | L-BFGS is 200 lines. The tuning is in the linear algebra underneath, which is linked. |
| ODE integrators | **Write** | Same. The published Butcher tableaux are the artefact. |
| Sequence alignment | **Write** | Smith-Waterman is a dynamic program; the tuned versions are SIMD, which F2 gives natively. |
| Units | **Write** — it is a type-system feature, not a library (§12) | |

Two consequences worth stating because they are easy to get wrong later.

**Random numbers are not a place to save work.** Linking against a C PRNG
forfeits the one guarantee §1 advertises. The counter-based designs (Philox,
Threefry) are a few dozen lines, are splittable without communication, and are
what JAX uses precisely because a splittable key is checkable. Write them.

**The BLAS binding is already specified.** `ffi-c-boundary.md` §1.6 defines the
two `extern` blocks for LP64 and ILP64 and §1.7 writes the safe `dgemm` wrapper.
`linalg` is the consumer of that work, not a second design of it. Where this note
names a `linalg` function, the implementation is a safe wrapper of the shape the
FFI note already demonstrates.

---

## 3. Names against the reserved list

Every name in this catalogue was checked against §13 and against
`crates/science-lexer/src/token.rs`. The collisions, and what the catalogue does
about them:

| Wanted | Reserved | Catalogue uses | If `reserved-words.md` lands |
|---|---|---|---|
| `mod(a, b)` | yes | `remainder(a, b)` | revert to `mod` |
| `kernel` (KDE, convolution) | yes | `window` / `smoother` | revert to `kernel` |
| `union` (sets) | yes | `merged` | revert to `union` |
| `yield` (reaction) | yes | `produced` / `efficiency` | revert to `yield` |
| `any(mask)` | keyword | `any_of` / `all_of` | keep `any_of`; symmetry is worth more |
| `match` (regex) | keyword | `find` / `matches` | keep — better names anyway |
| `at` (`poly.at(x)`) | keyword | `poly.evaluate(x)` | `at` after the dot rule |
| `model` (statistical) | yes | `Fit`, `fitted` | revert to `model` |
| `shape` (of an array) | yes | `dims` | revert to `shape` |
| `const` (module) | keyword | `physics.constants` | keep — the long name is clearer |
| `assert` | yes | `check` | keep reserved, per `reserved-words.md` §4.2 |

`stats` is the module that suffers most: **`model` is its central noun.** A
statistics library whose fitted-model type cannot be called a model, and whose
users cannot write `let model be fit(…)`, is paying for the reservation on every
line. This is the strongest single piece of evidence for `reserved-words.md` §3.3.

Safe throughout: `grad`, `dim`, `dims`, `axis`, `device`, `dtype`, `unit`,
`alias`, and every module name proposed here.

---

## 4. What waits for F1

The dividing line is the array type, not the mathematics.

**F0 today** — everything scalar, and everything over `Array of F64`:

- all of `math` except the array-shaped reductions
- `physics.constants` and `physics.units` (the units *checking* needs §14)
- `chem`'s stoichiometry, formula parsing, periodic table
- `bio`'s sequence work in full
- the scalar parts of `stats`: summary statistics, distributions, tests
- `optimize`'s scalar root finding and 1-D minimisation

**F1, because it needs `Tensor` with typed shapes:**

- all of `linalg` — a matrix is a rank-2 tensor whose shape is checked
- `signal` in full — an FFT's output shape is a function of its input shape
- multivariate `stats`: regression, PCA, covariance
- multivariate `optimize`: everything with a gradient vector
- `chem`'s spectra and molecular dynamics
- `bio`'s expression matrices and population structure

The rule of thumb: **if the function's correctness depends on two arrays having
compatible shapes, it waits for F1**, because writing it in F0 means checking at
runtime what F1 checks at compile time, and then rewriting it.

---

## 5. `math`

No dependencies. The foundation everything else calls. Overwhelmingly F0-able.

### 5.1 Elementary

`sqrt` `cbrt` `hypot` `exp` `exp2` `exp10` `expm1` `ln` `log2` `log10` `ln1p`
`log_base` `pow` `abs` `sign` `copysign` `floor` `ceil` `round` `trunc` `fract`
`remainder` `rem_euclid` `div_euclid` `min` `max` `clamp` `lerp` `fma`
`next_after` `ulp`

Trigonometry: `sin` `cos` `tan` `asin` `acos` `atan` `atan2` `sin_cos`
`sinh` `cosh` `tanh` `asinh` `acosh` `atanh` `degrees` `radians`

Predicates: `is_nan` `is_infinite` `is_finite` `is_normal` `is_close`
`is_close_within`

### 5.2 Special functions

Gamma family: `gamma` `ln_gamma` `sign_gamma` `digamma` `polygamma` `beta`
`ln_beta` `factorial` `ln_factorial` `rising_factorial` `falling_factorial`

Error family: `erf` `erfc` `erf_inverse` `erfc_inverse` `dawson` `faddeeva`

Incomplete: `gamma_lower` `gamma_upper` `gamma_regularised` `beta_incomplete`
`beta_regularised`

Bessel: `bessel_j` `bessel_y` `bessel_i` `bessel_k` `bessel_j_zero`
`spherical_bessel_j` `spherical_bessel_y` `airy_ai` `airy_bi`

Orthogonal polynomials: `legendre` `associated_legendre` `chebyshev_first`
`chebyshev_second` `hermite` `hermite_probabilists` `laguerre`
`associated_laguerre` `jacobi` `gegenbauer` `spherical_harmonic`

Elliptic: `elliptic_k` `elliptic_e` `elliptic_f` `elliptic_pi`
`jacobi_elliptic_sn` `jacobi_elliptic_cn` `jacobi_elliptic_dn`

Zeta and friends: `zeta` `hurwitz_zeta` `polylog` `dilog` `lambert_w`
`exponential_integral` `sine_integral` `cosine_integral` `fresnel_s` `fresnel_c`

Hypergeometric: `hypergeometric_0f1` `hypergeometric_1f1`
`hypergeometric_2f1` `confluent_u`

### 5.3 Calculus

Differentiation: `derivative` `derivative_order` `gradient_numeric`
`jacobian_numeric` `hessian_numeric` `richardson_extrapolate`

Quadrature: `integrate` `integrate_adaptive` `integrate_infinite`
`integrate_singular` `gauss_legendre` `gauss_hermite` `gauss_laguerre`
`gauss_chebyshev` `clenshaw_curtis` `tanh_sinh` `romberg` `simpson`
`trapezoid` `integrate_2d` `integrate_nd` `monte_carlo_integrate`

Series: `series_sum` `aitken_accelerate` `levin_accelerate`
`continued_fraction`

### 5.4 Ordinary differential equations

Explicit: `rk4` `rk45_dormand_prince` `rk23_bogacki_shampine`
`rk8_dormand_prince` `adams_bashforth` `verner`

Implicit and stiff: `bdf` `radau_iia` `rosenbrock` `trbdf2`
`backward_euler` `implicit_midpoint`

Symplectic, for Hamiltonian systems: `leapfrog` `velocity_verlet`
`forest_ruth` `yoshida4`

Support: `OdeProblem` `OdeSolution` `StepControl` `Tolerance` `EventHandler`
`solve_ode` `solve_ode_dense` `solve_bvp` `solve_dae` `shooting_method`
`collocation`

### 5.5 Interpolation

`linear_interpolate` `nearest_interpolate` `cubic_spline` `natural_spline`
`clamped_spline` `not_a_knot_spline` `monotone_cubic` `akima_spline`
`pchip` `bspline` `bspline_basis` `barycentric` `lagrange_interpolate`
`newton_divided_difference` `rational_interpolate` `thin_plate_spline`
`radial_basis` `bilinear_interpolate` `bicubic_interpolate` `regular_grid`
`scattered_interpolate`

### 5.6 Polynomials

Type `Polynomial of T`, with: `evaluate` `evaluate_horner` `derivative`
`antiderivative` `add` `subtract` `multiply` `divide` `remainder_of` `gcd`
`compose` `shift` `scale` `reverse` `degree` `coefficients` `leading`
`is_zero` `from_roots` `roots` `companion_matrix` `discriminant`
`resultant` `sturm_sequence` `count_real_roots`

Bases: `chebyshev_fit` `chebyshev_to_power` `power_to_chebyshev`
`legendre_fit` `orthogonal_fit`

### 5.7 Root finding

`bisect` `false_position` `ridders` `brent` `newton` `newton_safe` `halley`
`secant` `steffensen` `illinois` `toms748` `aberth_ehrlich` `jenkins_traub`
`durand_kerner` `find_all_roots` `bracket_root`

### 5.8 Number theory and combinatorics

Number theory: `gcd` `lcm` `extended_gcd` `mod_inverse` `mod_pow` `is_prime`
`is_probable_prime` `next_prime` `previous_prime` `primes_below` `sieve`
`factorise` `divisors` `divisor_count` `divisor_sum` `euler_totient`
`mobius` `carmichael` `jacobi_symbol` `legendre_symbol` `chinese_remainder`
`continued_fraction_of` `convergents` `rational_approximate`

Combinatorics: `binomial` `multinomial` `permutations_count`
`combinations_count` `catalan` `bell` `stirling_first` `stirling_second`
`partitions` `derangements` `fibonacci` `lucas` `bernoulli_number`
`euler_number` `permutations` `combinations` `power_set` `cartesian_product`
`multiset_permutations`

### 5.9 Reductions

These are what `data-io.md` §11.4 is waiting for. Free functions and chain
adaptors both, per `collections-and-chains.md`.

`sum` `product` `mean` `minimum` `maximum` `argmin` `argmax` `cumulative_sum`
`cumulative_product` `differences` `any_of` `all_of` `count_where`
`sum_compensated` `dot` `norm` `normalise`

`sum_compensated` is Kahan summation and is separate on purpose: naive `sum` over
a large `F32` array is one of the most common silent errors in scientific code,
and the library should make the accurate one nameable rather than choosing for
the user.

### 5.10 Complex numbers

Type `Complex of T`: `real` `imaginary` `conjugate` `modulus` `argument`
`modulus_squared` `from_polar` `to_polar` `exp` `ln` `sqrt` `pow` `sin` `cos`
`tan` `sinh` `cosh` `tanh` `asin` `acos` `atan` `is_nan` `is_finite`

`Complex` implements `Add`, `Sub`, `Mul`, `Div`, `Neg`, `Eq`, `Display` — the
operator traits of §5.4 are exactly what makes this readable.

### 5.11 Five signatures

```science
def sqrt(x: F64) returns F64

# Quadrature over a closure. `def(F64) returns F64` as a type is the
# spelling `ffi-c-boundary.md` §10.1 flags as unsettled in the core spec; this
# note assumes it and depends on that question being answered.
def integrate(
    f: def(F64) returns F64,
    lower: F64,
    upper: F64,
) -> (F64, QuadratureError?)

# Generic over the float width, which is why it is written in Science and not
# linked: a C `erf` is F64 only.
def erf of T(x: T) returns T where T: Float

# Polynomials carry their coefficient type. `evaluate` rather than `at`,
# because `at` is reserved by the comparison phrases (§3).
Polynomial of T has methods:
    def evaluate(self, x: T) returns T where T: Float

# An ODE solve returns an error because a stiff problem can fail to converge,
# and §8's rule is that nothing panics where an error will do.
def solve_ode of T(
    problem: borrowed OdeProblem of T,
    method: Method,
    tolerance: Tolerance,
) -> (OdeSolution of T, OdeError?)
```

---

## 6. `linalg`

Depends on `math`. **F1**, because every operation here is shape-checked.
Implementation is a safe wrapper over BLAS and LAPACK, per `ffi-c-boundary.md`.

### 6.1 Types

`Matrix of (T, const ROWS: Int, const COLS: Int)` `Vector of (T, const N: Int)`
`MatrixView` `Symmetric` `Hermitian` `Triangular` `Banded` `Diagonal`
`Sparse` `SparseCsr` `SparseCsc` `SparseCoo` `Permutation`

### 6.2 BLAS level 1, 2, 3

Level 1: `dot` `dotc` `axpy` `scal` `copy` `swap` `nrm2` `asum` `iamax`
`rot` `rotg`
Level 2: `gemv` `symv` `hemv` `trmv` `trsv` `ger` `syr` `syr2` `gbmv`
Level 3: `gemm` `symm` `hemm` `syrk` `syr2k` `trmm` `trsm`

Exposed as operators where the operator trait fits — `a @ b` is `gemm` — and by
name where the BLAS options matter.

### 6.3 Decompositions

`lu` `lu_partial` `lu_full` `qr` `qr_pivoted` `qr_economy` `cholesky`
`cholesky_ldl` `svd` `svd_economy` `svd_truncated` `eigen` `eigen_symmetric`
`eigen_generalised` `schur` `hessenberg` `bidiagonal` `tridiagonal`
`polar` `rq` `ql` `lq` `cs`

### 6.4 Solving

`solve` `solve_triangular` `solve_symmetric` `solve_positive_definite`
`solve_least_squares` `solve_least_norm` `solve_banded` `solve_tridiagonal`
`solve_sparse` `lstsq` `nnls` `pinv` `inverse` `inverse_symmetric`

Iterative: `conjugate_gradient` `bicgstab` `gmres` `minres` `lsqr` `lsmr`
`preconditioner_jacobi` `preconditioner_ilu` `preconditioner_ichol`

### 6.5 Properties and functions

`determinant` `log_determinant` `sign_determinant` `trace` `rank` `norm`
`norm_frobenius` `norm_spectral` `norm_nuclear` `condition_number`
`is_symmetric` `is_positive_definite` `is_orthogonal` `is_singular`

Matrix functions: `matrix_exp` `matrix_log` `matrix_sqrt` `matrix_power`
`matrix_sign` `matrix_cos` `matrix_sin`

### 6.6 Structure

`transpose` `conjugate_transpose` `reshape` `kron` `outer` `block_diagonal`
`horizontal_stack` `vertical_stack` `triangular_upper` `triangular_lower`
`diagonal_of` `from_diagonal` `identity` `zeros` `ones` `full` `eye`
`companion` `vandermonde` `hilbert` `toeplitz` `hankel` `circulant`

### 6.7 Five signatures

```science
# The shape is in the type. This is the whole reason linalg waits for F1: a
# dimension mismatch is a compile error, not a runtime one.
def matmul of (T, const M: Int, const K: Int, const N: Int)(
    left: borrowed Matrix of (T, M, K),
    right: borrowed Matrix of (T, K, N),
) returns Matrix of (T, M, N) where T: Float

# A solve can fail on a singular matrix, so it returns an error beside the
# value. The shapes still make a dimension mismatch impossible to write.
def solve of (T, const N: Int)(
    a: borrowed Matrix of (T, N, N),
    b: borrowed Vector of (T, N),
) -> (Vector of (T, N), LinalgError?) where T: Float

# Cholesky's precondition — positive definiteness — is not in the type, and
# cannot be. It is the error.
def cholesky of (T, const N: Int)(
    a: borrowed Symmetric of (T, N),
) -> (Triangular of (T, N), NotPositiveDefinite?) where T: Float

# The economy SVD's output shapes are a function of the input's, which is the
# case const-generic arithmetic has to handle (§14.1).
def svd_economy of (T, const M: Int, const N: Int)(
    a: borrowed Matrix of (T, M, N),
) -> (Svd of (T, M, N), LinalgError?) where T: Float

def conjugate_gradient of (T, const N: Int)(
    a: borrowed Sparse of (T, N, N),
    b: borrowed Vector of (T, N),
    tolerance: T,
    limit: Int,
) -> (Vector of (T, N), DidNotConverge?) where T: Float
```

---

## 7. `stats`

Depends on `linalg`. The module most damaged by the reserved-word list (§3).

### 7.1 Descriptive

`mean` `median` `mode` `geometric_mean` `harmonic_mean` `trimmed_mean`
`winsorised_mean` `weighted_mean` `variance` `variance_population`
`standard_deviation` `standard_error` `median_absolute_deviation`
`interquartile_range` `range_of` `skewness` `kurtosis` `excess_kurtosis`
`moment` `central_moment` `standardised_moment` `quantile` `percentile`
`quartiles` `five_number_summary` `covariance` `correlation`
`correlation_spearman` `correlation_kendall` `autocorrelation`
`partial_correlation` `covariance_matrix` `correlation_matrix`

### 7.2 Distributions

Each is a type implementing a `Distribution` trait with `pdf` `log_pdf` `cdf`
`log_cdf` `survival` `quantile` `sample` `mean` `variance` `skewness`
`kurtosis` `entropy` `support` `fit_to`.

Continuous: `Normal` `LogNormal` `HalfNormal` `Uniform` `Exponential` `Gamma`
`InverseGamma` `Beta` `ChiSquared` `InverseChiSquared` `StudentT` `Cauchy`
`Laplace` `Logistic` `Gumbel` `Frechet` `Weibull` `Pareto` `Rayleigh` `Rice`
`Levy` `Triangular` `VonMises` `Wald` `Kumaraswamy` `SkewNormal`

Discrete: `Bernoulli` `Binomial` `NegativeBinomial` `Geometric` `Poisson`
`Hypergeometric` `Categorical` `Multinomial` `DiscreteUniform` `Zipf`
`BetaBinomial`

Multivariate: `MultivariateNormal` `MultivariateT` `Dirichlet` `Wishart`
`InverseWishart` `MatrixNormal` `LKJ`

### 7.3 Random

`Key` `split` `split_many` `fold_in` — the splittable counter-based design of
§2. `uniform` `normal` `standard_normal` `exponential` `gamma` `beta` `poisson`
`binomial` `categorical` `permutation` `shuffle` `choice` `choice_weighted`
`bootstrap_sample` `sobol` `halton` `latin_hypercube`

> A generator function takes a `Key` and never mutates global state. §1 of the
> core spec names key reuse as a verification target; the API is what makes that
> checkable, and it is why this is written in Science (§2).

### 7.4 Hypothesis tests

`t_test_one_sample` `t_test_two_sample` `t_test_paired` `t_test_welch`
`z_test` `chi_squared_test` `chi_squared_goodness_of_fit` `fisher_exact`
`barnard_exact` `mcnemar` `binomial_test` `poisson_test`
`anova_one_way` `anova_two_way` `anova_repeated` `ancova` `manova`
`levene` `bartlett` `brown_forsythe` `fligner`
`mann_whitney` `wilcoxon_signed_rank` `kruskal_wallis` `friedman`
`sign_test` `mood_median` `ansari_bradley`
`shapiro_wilk` `anderson_darling` `kolmogorov_smirnov` `cramer_von_mises`
`jarque_bera` `dagostino_pearson` `lilliefors`
`durbin_watson` `ljung_box` `breusch_pagan` `white_test` `ramsey_reset`
`augmented_dickey_fuller` `kpss` `phillips_perron` `granger_causality`

Corrections: `bonferroni` `holm` `hochberg` `benjamini_hochberg`
`benjamini_yekutieli` `tukey_hsd` `dunnett` `scheffe` `sidak`

### 7.5 Regression and models

Because `model` is reserved (§3), the fitted object is a `Fit` and the verb is
`fit`.

`least_squares` `ridge` `lasso` `elastic_net` `least_angle`
`orthogonal_matching_pursuit` `principal_components_regression`
`partial_least_squares` `total_least_squares` `weighted_least_squares`
`generalised_least_squares` `robust_huber` `robust_tukey` `theil_sen` `ransac`
`quantile_regression`

Generalised linear: `glm` `logistic` `probit` `poisson_regression`
`negative_binomial_regression` `gamma_regression` `tweedie`

Mixed and hierarchical: `linear_mixed` `generalised_linear_mixed`
`variance_components`

Nonparametric: `local_regression` `smoothing_spline` `generalised_additive`
`isotonic` `nadaraya_watson`

Model assessment: `Fit` `residuals` `fitted_values` `leverage` `cooks_distance`
`dffits` `variance_inflation` `r_squared` `adjusted_r_squared` `aic` `aicc`
`bic` `deviance` `log_likelihood` `confidence_interval` `prediction_interval`
`cross_validate` `k_fold` `leave_one_out` `bootstrap_confidence`

### 7.6 Tables

Claimed here per §0.2, operating on `data-io.md`'s `Frame of R`.

`group_by` `aggregate` `summarise` `join_inner` `join_left` `join_right`
`join_outer` `join_cross` `pivot_wider` `pivot_longer` `melt` `crosstab`
`contingency_table` `sort_by` `rank` `dense_rank` `rolling` `expanding`
`lag` `lead` `difference` `resample` `fill_missing` `drop_missing`
`interpolate_missing`

### 7.7 Multivariate and unsupervised

`principal_components` `kernel_pca` `factor_analysis`
`independent_components` `canonical_correlation` `linear_discriminant`
`multidimensional_scaling` `isomap` `locally_linear` `umap` `tsne`
`k_means` `k_medoids` `hierarchical_cluster` `dbscan` `optics`
`gaussian_mixture` `spectral_cluster` `affinity_propagation`
`silhouette` `davies_bouldin` `calinski_harabasz` `adjusted_rand`

### 7.8 Time series

`acf` `pacf` `ccf` `periodogram` `welch_periodogram` `arima` `sarima`
`arimax` `var_model` `vecm` `state_space` `kalman_filter` `kalman_smoother`
`particle_filter` `exponential_smoothing` `holt_winters` `stl_decompose`
`x13_decompose` `hodrick_prescott` `hurst_exponent` `detrend`
`seasonal_adjust` `change_point`

### 7.9 Five signatures

```science
def mean of T(values: borrowed Array of T) -> (T, EmptyInput?)
    where T: Float

# `Distribution` is a trait with an associated type for the sample, which is
# what lets a discrete distribution sample an Int and a continuous one an F64.
trait Distribution:
    type Sample
    def pdf(self, x: Self.Sample) returns F64
    def cdf(self, x: Self.Sample) returns F64
    # By value, not borrowed. Consuming the key is the entire mechanism: a
    # borrowed key could be used twice, and §1 of the core spec names a reused
    # random key as one of the three things the language exists to catch.
    def sample(self, key: Key) returns Self.Sample

# The key is taken by value, not borrowed: consuming it is what makes reuse a
# compile error rather than a convention (§1 of the core spec).
def normal(key: Key, mean: F64, standard_deviation: F64) returns F64

def t_test_two_sample(
    first: borrowed Array of F64,
    second: borrowed Array of F64,
    alternative: Alternative,
) -> (TestResult, StatsError?)

# `fit`, not `model`. F1, because the design matrix's shape is checked.
def least_squares of (const N: Int, const P: Int)(
    design: borrowed Matrix of (F64, N, P),
    response: borrowed Vector of (F64, N),
) -> (Fit of P, LinalgError?)
```

---

## 8. `optimize`

Depends on `linalg` and `math`. Scalar parts are F0; everything with a gradient
vector is F1.

### 8.1 Unconstrained

`nelder_mead` `powell` `gradient_descent` `momentum` `nesterov` `adagrad`
`rmsprop` `adam` `adamw` `lbfgs` `bfgs` `dfp` `sr1` `newton_optimise`
`trust_region` `trust_region_dogleg` `trust_region_steihaug`
`conjugate_gradient_optimise` `fletcher_reeves` `polak_ribiere`
`line_search_armijo` `line_search_wolfe` `line_search_strong_wolfe`

### 8.2 Constrained

`linear_program` `simplex` `interior_point` `quadratic_program`
`sequential_quadratic` `augmented_lagrangian` `penalty_method`
`barrier_method` `projected_gradient` `active_set` `admm` `osqp_solve`
`second_order_cone` `semidefinite_program` `mixed_integer_linear`
`branch_and_bound` `cutting_plane`

### 8.3 Least squares and curve fitting

`levenberg_marquardt` `gauss_newton` `trust_region_reflective`
`curve_fit` `orthogonal_distance_regression` `bounded_least_squares`

### 8.4 Global and derivative-free

`differential_evolution` `particle_swarm` `simulated_annealing`
`basin_hopping` `dual_annealing` `cma_es` `direct` `shgo` `bayesian_optimise`
`gaussian_process_surrogate` `random_search` `grid_search` `cobyla` `bobyqa`

### 8.5 Scalar, and root finding on systems

`minimise_scalar` `golden_section` `brent_minimise` `bounded_scalar`
`fsolve` `broyden_good` `broyden_bad` `anderson_mixing` `newton_krylov`
`homotopy_continuation`

### 8.6 Five signatures

```science
def minimise_scalar(
    f: def(F64) returns F64,
    bracket: Bracket,
    tolerance: F64,
) -> (Minimum, DidNotConverge?)

# The gradient is optional: given one, lbfgs uses it; without one it falls back
# to finite differences, which is a decision the caller should see in the type.
def lbfgs of (const N: Int)(
    objective: def(borrowed Vector of (F64, N)) returns F64,
    gradient: (def(borrowed Vector of (F64, N)) -> Vector of (F64, N))?,
    start: Vector of (F64, N),
    settings: borrowed Settings,
) -> (Solution of N, OptimiseError?)

def curve_fit of (const N: Int, const P: Int)(
    f: def(F64, borrowed Vector of (F64, P)) returns F64,
    xs: borrowed Vector of (F64, N),
    ys: borrowed Vector of (F64, N),
    start: Vector of (F64, P),
) -> (Fit of P, OptimiseError?)

def linear_program of (const M: Int, const N: Int)(
    objective: borrowed Vector of (F64, N),
    constraints: borrowed Matrix of (F64, M, N),
    bounds: borrowed Vector of (F64, M),
) -> (Vector of (F64, N), Infeasible?)

def differential_evolution of (const N: Int)(
    objective: def(borrowed Vector of (F64, N)) returns F64,
    bounds: borrowed Array of Bounds,
    key: Key,
    settings: borrowed Settings,
) -> (Solution of N, OptimiseError?)
```

---

## 9. `signal`

Depends on `math` and a linked FFT (§2). **F1** throughout: every output shape
is a function of an input shape.

### 9.1 Transforms

`fft` `ifft` `rfft` `irfft` `fft2` `ifft2` `fftn` `ifftn` `fftshift`
`ifftshift` `fftfreq` `rfftfreq` `dct` `idct` `dst` `idst` `hilbert`
`analytic_signal` `czt` `zoom_fft` `stft` `istft` `spectrogram`
`wavelet_continuous` `wavelet_discrete` `wavelet_packet` `cwt_morlet`
`hartley` `mellin`

### 9.2 Filtering

Design: `butterworth` `chebyshev_one` `chebyshev_two` `elliptic` `bessel_filter`
`fir_window` `fir_least_squares` `remez` `savitzky_golay_coefficients`
`notch` `peak_filter` `all_pass` `shelf_low` `shelf_high`

Application: `filter_forward` `filter_forward_backward` `lfilter` `sosfilt`
`sosfiltfilt` `convolve` `convolve_fft` `correlate` `correlate_fft`
`deconvolve` `wiener` `median_filter` `rank_filter` `gaussian_filter`
`bilateral_filter` `savitzky_golay` `detrend_signal` `decimate` `resample_poly`
`upfirdn`

Windows — `window` rather than `kernel` (§3): `window_hann` `window_hamming`
`window_blackman` `window_blackman_harris` `window_bartlett` `window_kaiser`
`window_tukey` `window_gaussian` `window_flat_top` `window_nuttall`
`window_dpss` `window_chebyshev`

### 9.3 Analysis

`find_peaks` `peak_prominence` `peak_widths` `find_valleys` `zero_crossings`
`envelope` `instantaneous_frequency` `group_delay` `phase_delay`
`frequency_response` `impulse_response` `step_response` `bode` `nyquist_plot`
`coherence` `cross_spectral_density` `power_spectral_density` `welch`
`bartlett_psd` `multitaper` `lomb_scargle` `cepstrum` `mfcc`

### 9.4 Five signatures

```science
# The output length of an rfft is N/2+1 — the clearest case in the catalogue
# for const-generic arithmetic (§14.1). Without it this signature cannot be
# written and the length becomes a runtime value.
def rfft of (const N: Int)(
    signal: borrowed Vector of (F64, N),
) returns Vector of (Complex of F64, N / 2 + 1)

def fft of (const N: Int)(
    signal: borrowed Vector of (Complex of F64, N),
) returns Vector of (Complex of F64, N)

def butterworth(
    order: Int,
    cutoff: F64,
    kind: BandKind,
    sample_rate: F64,
) -> (SecondOrderSections, FilterError?)

def filter_forward_backward of (const N: Int)(
    sections: borrowed SecondOrderSections,
    signal: borrowed Vector of (F64, N),
) returns Vector of (F64, N)

def find_peaks of (const N: Int)(
    signal: borrowed Vector of (F64, N),
    settings: borrowed PeakSettings,
) returns Array of Peak
```

---

## 10. `chem`

Depends on `physics.units` and `optimize`. Substantially F0-able — stoichiometry
and formula parsing are string and integer work.

### 10.1 Elements and formulas

`Element` `Isotope` `PeriodicTable` `Formula` `Compound`
`element_by_symbol` `element_by_number` `element_by_name` `isotopes_of`
`atomic_mass` `atomic_number` `electron_configuration` `electronegativity`
`covalent_radius` `van_der_waals_radius` `ionisation_energy`
`electron_affinity` `oxidation_states` `parse_formula` `format_formula`
`molar_mass` `mass_fractions` `empirical_formula` `molecular_formula`
`isotope_pattern` `exact_mass` `nominal_mass` `degree_of_unsaturation`

### 10.2 Stoichiometry

`Reaction` `parse_reaction` `balance` `is_balanced` `limiting_reagent`
`theoretical_produced` `percent_produced` `excess_reagent`
`stoichiometric_ratio` `moles_from_mass` `mass_from_moles`
`moles_from_volume` `molarity` `molality` `mole_fraction` `mass_percent`
`parts_per_million` `dilution` `titration_point`

> `theoretical_produced` and `percent_produced` are the names §3 forces:
> `yield` is reserved. This is the single most-visible cost of that
> reservation anywhere in the catalogue — *yield* is the word the entire
> discipline uses.

### 10.3 Thermodynamics

`enthalpy_of_formation` `entropy_standard` `gibbs_free_energy`
`gibbs_from_enthalpy_entropy` `heat_capacity` `heat_capacity_shomate`
`enthalpy_of_reaction` `entropy_of_reaction` `gibbs_of_reaction`
`equilibrium_constant` `van_t_hoff` `clausius_clapeyron` `hess_law`
`bond_enthalpy` `lattice_energy` `born_haber` `adiabatic_flame_temperature`
`joule_thomson` `fugacity` `activity_coefficient` `raoult` `henry_law`
`colligative_freezing` `colligative_boiling` `osmotic_pressure`

### 10.4 Kinetics

`rate_law` `rate_constant` `arrhenius` `eyring` `half_life`
`integrated_zero_order` `integrated_first_order` `integrated_second_order`
`michaelis_menten` `lineweaver_burk` `hanes_woolf` `eadie_hofstee`
`hill_equation` `competitive_inhibition` `noncompetitive_inhibition`
`uncompetitive_inhibition` `steady_state` `pre_equilibrium`
`collision_theory` `transition_state` `reaction_network` `solve_kinetics`

### 10.5 Equilibrium and solutions

`equilibrium_composition` `ice_table` `reaction_quotient` `le_chatelier_shift`
`acid_dissociation` `base_dissociation` `ph` `poh` `p_ka` `p_kb`
`henderson_hasselbalch` `buffer_capacity` `titration_curve`
`equivalence_point` `solubility_product` `common_ion` `complex_formation`
`nernst` `cell_potential` `standard_potential` `faraday_electrolysis`

### 10.6 Structure and spectroscopy

`Molecule` `Atom` `Bond` `Conformer` `read_xyz` `read_pdb` `read_mol`
`read_sdf` `write_xyz` `smiles_parse` `smiles_write` `inchi`
`bond_length` `bond_angle` `dihedral` `centre_of_mass` `moment_of_inertia`
`rotational_constants` `radius_of_gyration` `rmsd` `align_structures`
`beer_lambert` `absorbance` `transmittance` `peak_assign`
`nmr_chemical_shift` `nmr_coupling` `ir_frequencies` `ms_fragment`
`uv_vis_gap`

### 10.7 Five signatures

```science
# Balancing is a nullspace computation over the element-count matrix, which is
# why chem depends on linalg transitively. It fails on an unbalanceable input.
def balance(reaction: borrowed Reaction) -> (Reaction, BalanceError?)

# Molar mass carries its unit in the type. This is the payoff of §12: a molar
# mass cannot be added to a mass, and the compiler says so.
def molar_mass(formula: borrowed Formula) returns Quantity of (F64, GramsPerMole)

def arrhenius(
    activation_energy: Quantity of (F64, JoulesPerMole),
    temperature: Quantity of (F64, Kelvin),
    pre_exponential: F64,
) returns F64

# `produced`, not `yield` (§3).
def percent_produced(
    actual: Quantity of (F64, Grams),
    theoretical: Quantity of (F64, Grams),
) returns F64

def ph(concentration: Quantity of (F64, MolesPerLitre)) returns F64
```

---

## 11. `bio`

Depends on `stats`. Sequence work is fully F0-able and is the part with the
clearest early payoff.

### 11.1 Sequences

`DnaSequence` `RnaSequence` `ProteinSequence` `Alphabet` `Codon`
`complement` `reverse_complement` `transcribe` `back_transcribe` `translate`
`translate_frame` `open_reading_frames` `gc_content` `gc_skew` `at_content`
`melting_temperature` `molecular_weight_sequence` `isoelectric_point`
`hydropathy` `codon_usage` `codon_adaptation_index` `shuffle_sequence`
`kmer_counts` `kmer_spectrum` `minimiser` `entropy_of_sequence`

### 11.2 Alignment

`align_global` `align_local` `align_semiglobal` `align_overlap`
`needleman_wunsch` `smith_waterman` `gotoh` `hirschberg` `align_affine`
`align_multiple` `progressive_align` `profile_align` `pairwise_identity`
`edit_distance` `hamming_distance` `levenshtein` `substitution_matrix`
`blosum` `pam` `dotplot` `find_motif` `position_weight_matrix` `score_motif`
`consensus`

### 11.3 Phylogenetics

`Tree` `Clade` `neighbour_joining` `upgma` `maximum_parsimony`
`maximum_likelihood_tree` `bayesian_tree` `distance_matrix`
`jukes_cantor` `kimura_two_parameter` `tamura_nei` `general_time_reversible`
`bootstrap_tree` `consensus_tree` `robinson_foulds` `tree_length`
`root_at_midpoint` `ladderise` `read_newick` `write_newick`
`ancestral_state` `molecular_clock` `coalescent`

### 11.4 Population genetics

`allele_frequency` `genotype_frequency` `hardy_weinberg` `heterozygosity`
`inbreeding_coefficient` `fst` `gst` `nucleotide_diversity` `watterson_theta`
`tajima_d` `fu_li_d` `fay_wu_h` `linkage_disequilibrium` `r_squared_ld`
`recombination_rate` `effective_population_size` `selection_coefficient`
`mcdonald_kreitman` `site_frequency_spectrum` `wright_fisher`
`admixture_proportions`

### 11.5 Expression and omics

`ExpressionMatrix` `normalise_counts` `counts_per_million` `tpm` `fpkm`
`rlog` `variance_stabilising` `quantile_normalise` `batch_correct`
`differential_expression` `deseq_like` `edger_like` `limma_like`
`volcano_data` `ma_data` `enrichment_analysis` `gene_set_enrichment`
`over_representation` `pathway_score` `cluster_samples` `cluster_genes`
`marker_genes` `pseudotime` `cell_type_score`

### 11.6 Structure

`ProteinStructure` `read_pdb_structure` `read_mmcif` `secondary_structure`
`solvent_accessibility` `contact_map` `distance_map` `ramachandran`
`superimpose` `rmsd_structure` `tm_score` `gdt_score` `radius_of_gyration`
`hydrogen_bonds` `salt_bridges` `binding_site`

### 11.7 Five signatures

```science
# A DNA sequence and a protein sequence are different types, so translating in
# the wrong direction is a compile error rather than nonsense output.
def translate(
    sequence: borrowed DnaSequence,
    table: GeneticCode,
) -> (ProteinSequence, TranslationError?)

def reverse_complement(sequence: borrowed DnaSequence) returns DnaSequence

def smith_waterman of T(
    first: borrowed T,
    second: borrowed T,
    scoring: borrowed Scoring,
) returns Alignment where T: Sequence

def neighbour_joining of (const N: Int)(
    distances: borrowed Symmetric of (F64, N),
    labels: borrowed Array of String,
) -> (Tree, TreeError?)

def tajima_d(
    sequences: borrowed Array of DnaSequence,
) -> (F64, TooFewSequences?)
```

---

## 12. `physics`, and the units decision

The transversal question, answered rather than deferred.

### 12.1 The catalogue first

**`physics.constants`** — named `constants`, not `const`, which is a keyword
(§3). CODATA values, each a `Quantity` with its unit and its uncertainty:
`SPEED_OF_LIGHT` `PLANCK` `REDUCED_PLANCK` `GRAVITATIONAL` `ELEMENTARY_CHARGE`
`ELECTRON_MASS` `PROTON_MASS` `NEUTRON_MASS` `ATOMIC_MASS_UNIT` `AVOGADRO`
`BOLTZMANN` `GAS_CONSTANT` `FARADAY` `STEFAN_BOLTZMANN` `WIEN_DISPLACEMENT`
`RYDBERG` `BOHR_RADIUS` `BOHR_MAGNETON` `NUCLEAR_MAGNETON` `FINE_STRUCTURE`
`VACUUM_PERMITTIVITY` `VACUUM_PERMEABILITY` `VACUUM_IMPEDANCE`
`STANDARD_GRAVITY` `STANDARD_ATMOSPHERE`

**`physics.mechanics`** — `kinematics_position` `kinematics_velocity`
`projectile_range` `projectile_apex` `momentum` `impulse` `kinetic_energy`
`potential_energy_gravitational` `potential_energy_spring` `work` `power`
`centre_of_mass` `moment_of_inertia` `angular_momentum` `torque`
`rigid_body_step` `orbital_period` `escape_velocity` `vis_viva`
`kepler_solve` `two_body` `n_body_step` `lagrange_points` `hohmann_transfer`
`damped_oscillator` `driven_oscillator` `pendulum_period` `normal_modes`

**`physics.thermo`** — `ideal_gas_pressure` `ideal_gas_volume`
`van_der_waals` `redlich_kwong` `peng_robinson` `compressibility`
`carnot_efficiency` `otto_efficiency` `diesel_efficiency` `rankine`
`heat_transfer_conduction` `heat_transfer_convection` `heat_transfer_radiation`
`blackbody_spectral` `stefan_boltzmann_power` `wien_peak` `entropy_change`
`maxwell_boltzmann_speed` `partition_function` `fermi_dirac` `bose_einstein`

**`physics.em`** — `coulomb_force` `electric_field_point` `electric_potential`
`gauss_flux` `capacitance_parallel` `capacitance_cylindrical` `energy_stored`
`current_density` `resistance` `resistivity` `ohm` `power_dissipated`
`biot_savart` `ampere_loop` `magnetic_force` `lorentz_force` `cyclotron_radius`
`faraday_induction` `inductance` `lc_frequency` `rlc_response` `impedance`
`poynting` `wave_impedance` `snell` `fresnel_coefficients` `brewster_angle`
`critical_angle` `diffraction_grating` `rayleigh_criterion`

**`physics.quantum`** — `de_broglie` `photon_energy` `photon_momentum`
`compton_shift` `photoelectric` `bohr_energy` `bohr_radius_n`
`rydberg_transition` `particle_in_box` `harmonic_oscillator_levels`
`hydrogen_wavefunction` `uncertainty_product` `tunnelling_probability`
`spin_expectation` `pauli_matrices` `clebsch_gordan` `wigner_3j` `wigner_6j`
`density_matrix` `von_neumann_entropy`

**`physics.relativity`** — `lorentz_factor` `time_dilation` `length_contraction`
`relativistic_momentum` `relativistic_energy` `rest_energy`
`velocity_addition` `doppler_relativistic` `four_vector` `minkowski_interval`
`schwarzschild_radius` `gravitational_redshift` `geodesic_step`

**`physics.units`** — the type machinery, §12.2 onward.

### 12.2 The decision

**Yes. Units belong in the type, with static dimensional checking.**

The argument is §1 of the core spec, which names the bet:

> Julia has the REPL and the ecosystem and cannot check a shape, a unit, or a
> reused random key before running.

Units are one of the three things the language exists to check. A `physics`
module whose quantities are bare `F64` would make the spec's own example of
Julia's weakness true of Science as well. Mars Climate Orbiter is the canonical
loss, and it is a *type error* that a type system declined to have an opinion
about.

So the question is not whether, it is how, and the how is where the cost is.

### 12.3 The representation

A dimension is a vector of seven integer exponents over the SI base dimensions:
length, mass, time, electric current, thermodynamic temperature, amount of
substance, luminous intensity.

```science
type Quantity of (
    T,
    const LENGTH: Int,
    const MASS: Int,
    const TIME: Int,
    const CURRENT: Int,
    const TEMPERATURE: Int,
    const AMOUNT: Int,
    const LUMINOUS: Int,
):
    value: T
```

with aliases for what people actually write:

```science
type Length of T is Quantity of (T, 1, 0, 0, 0, 0, 0, 0)
type Mass of T is Quantity of (T, 0, 1, 0, 0, 0, 0, 0)
type Time of T is Quantity of (T, 0, 0, 1, 0, 0, 0, 0)
type Velocity of T is Quantity of (T, 1, 0, -1, 0, 0, 0, 0)
type Acceleration of T is Quantity of (T, 1, 0, -2, 0, 0, 0, 0)
type Force of T is Quantity of (T, 1, 1, -2, 0, 0, 0, 0)
type Energy of T is Quantity of (T, 2, 1, -2, 0, 0, 0, 0)
type Power of T is Quantity of (T, 2, 1, -3, 0, 0, 0, 0)
type Pressure of T is Quantity of (T, -1, 1, -2, 0, 0, 0, 0)
```

This is the design `uom` uses in Rust and F# uses natively, and it is the only
one that is **open**: a user can form a dimension nobody predicted —
`kg·m²·s⁻³·A⁻²` is electrical resistance and also whatever the fifth term of
someone's correlation happens to be — without the library having declared it.

### 12.4 What F0 lacks, precisely

Addition is easy: both operands must have identical exponents, which is ordinary
type equality and works today.

**Multiplication is the problem**, and it is the whole of the problem:

```science
Quantity implements Mul:
    def mul of (
        const L1: Int, const M1: Int, const T1: Int, …,
        const L2: Int, const M2: Int, const T2: Int, …,
    )(
        self: Quantity of (T, L1, M1, T1, …),
        other: Quantity of (T, L2, M2, T2, …),
    ) returns Quantity of (T, L1 + L2, M1 + M2, T1 + T2, …)
```

`L1 + L2` is arithmetic **in type position**. F0 has const generic *parameters*
— §5.3 gives `type Matrix of (T, const ROWS: Int, const COLS: Int)` — and no
const generic *expressions*. Three things are missing:

1. **Const expressions in type arguments.** The grammar must admit `L1 + L2`
   where a const argument is expected. `TypeKind::Const` holds a literal today.
2. **Evaluation.** The type checker must evaluate `+`, `-` and unary negation
   over integer const parameters. A small closed set — no division, no calls.
3. **Equality up to normalisation, which is the hard one.** Is
   `Quantity of (T, L1 + L2, …)` the same type as `Quantity of (T, L2 + L1, …)`?
   It must be, or `a * b` and `b * a` have different types and the library is
   unusable. So the checker needs a normal form for const expressions — sum of
   terms with integer coefficients, sorted — and equality decided on it.

Point 3 is where Rust's const generics stalled for years, and it should be
stated plainly rather than discovered later. But it is also *easier here than it
was for Rust*, because the expression language required is deliberately tiny:
linear integer combinations of parameters, nothing else. A normaliser over
`Σ cᵢ·pᵢ + k` is a few hundred lines and is decidable.

### 12.5 The argument that makes this affordable

**F1's tensor shapes need exactly the same feature.** Consider the signatures
this note has already had to write:

```science
def rfft of (const N: Int)(…) returns Vector of (Complex of F64, N / 2 + 1)
def concatenate of (const A: Int, const B: Int)(…) returns Tensor of (F32, A + B)
```

The `N / 2 + 1` in §9.4 and the `A + B` of any concatenation are the same
requirement as `L1 + L2`. §1 of the core spec bets on *shapes, units and random
keys*; two of those three need const-expression arithmetic, and they need the
same implementation of it.

So the recommendation is not "build a units feature". It is:

> **Build const-expression arithmetic once, in F1, because shapes need it
> anyway; units then cost a library and no further language work.**

That reframes the units question from a feature request into a consumer of work
already required, which is the only framing under which it is affordable.

One extension shapes will want and units will not: `N / 2 + 1` has a division,
so the normal form must handle division by a literal. Units need only linear
combinations. Design for the shape case and units come free.

### 12.6 What is deliberately excluded

- **Rational exponents.** `sqrt(area)` would need `Length^(1/2)`. Excluded:
  integer exponents only, and `sqrt` of a non-even dimension is a type error.
  The cost is real in a few corners of fluid dynamics and is worth the
  simplicity.
- **Affine units.** Celsius and Fahrenheit have offsets, so `20°C + 20°C` is
  meaningless while `20K + 20K` is fine. `Quantity` is linear only; Celsius is a
  separate `Temperature` type with explicit conversions, and never an arithmetic
  operand.
- **Unit *systems* as opposed to dimensions.** A `Length` is a length; whether it
  is displayed in metres or feet is a formatting concern, held in the value's
  scale at construction and conversion, not in its type. Putting the system in
  the type doubles the type count and buys a check nobody needs — mixing metres
  and feet is already impossible because both are `Length` and conversion is
  explicit at the boundary.

### 12.7 Five signatures

```science
def lorentz_factor(velocity: Velocity of F64) returns F64

# The return dimension is computed from the arguments'. This one line is what
# §12.4 is asking F0 for.
def kinetic_energy(mass: Mass of F64, velocity: Velocity of F64) returns Energy of F64

def schwarzschild_radius(mass: Mass of F64) returns Length of F64

def ideal_gas_pressure(
    amount: Quantity of (F64, 0, 0, 0, 0, 0, 1, 0),
    temperature: Quantity of (F64, 0, 0, 0, 0, 1, 0, 0),
    volume: Quantity of (F64, 3, 0, 0, 0, 0, 0, 0),
) returns Pressure of F64

# Conversion is explicit and is the only place a bare number becomes a quantity.
def metres(value: F64) returns Length of F64
```

---

## 13. Sequencing

What to build, in what order, and why.

| Wave | Contents | Gated on |
|---|---|---|
| **1** | `math` §5.1–5.2, §5.7–5.10; `stats` §7.1–7.3 scalar; `bio` §11.1–11.2 | Nothing. All F0 today. |
| **2** | `physics.constants`, `chem` §10.1–10.5 | Wave 1. Units *unchecked* — quantities as `F64` with a documented unit — so the module exists before §12 lands. |
| **3** | `linalg` in full | F1 tensors, and the BLAS bindings of `ffi-c-boundary.md`. |
| **4** | `stats` §7.5–7.8; `optimize`; `signal` | Wave 3. |
| **5** | `physics.units` retro-fitted through `chem` and `physics` | Const-expression arithmetic (§12.4). |
| **6** | `bio` §11.3–11.6 | Wave 4. |

Wave 2's compromise is deliberate and should be stated where users see it: a
`chem` that ships before units are checkable is more useful than no `chem`, and
the retrofit in wave 5 is a type change with no call-site changes, because the
function names and argument orders do not move.

---

## 14. What this note asks for

Ordered by how much is blocked behind each.

1. **Const-expression arithmetic in type position** (§12.4). Blocks units, and
   blocks any shape that is a function of another shape — `rfft`, concatenation,
   reshaping, convolution output sizes. The largest single ask, and F1 needs it
   regardless of units.
2. **Closure types as a spelling.** `def(F64) returns F64` appears in
   `integrate`, `minimise_scalar`, `lbfgs`, `curve_fit` and
   `differential_evolution`. `ffi-c-boundary.md` §10.1 already flags that the
   core spec defines closure *expressions* and never their types. Two notes now
   depend on it; it should be settled.
3. **Explicit type arguments at call sites.** `data-io.md` §11.2 asks for
   `read_csv of Measurement(...)`; this note needs the same for
   `(Array of Doc).new()`-shaped constructors and for `zeros of (F64, 3, 3)()`.
   One feature, two notes.
4. **A `Float` trait** covering `F32` and `F64`, since almost every signature
   here is generic over it. §5.4 of the core spec lists the library traits and
   does not include one.
5. **The reserved-word decisions of `reserved-words.md`.** `stats` cannot call a
   fitted model a `model`, `chem` cannot call a yield a `yield`, and `signal`
   cannot call a kernel a `kernel`. §3 lists eleven renames forced by the current
   list; eight of them disappear if that note's recommendation is taken.
6. **An uncertainty type.** CODATA constants carry uncertainties, and every
   measured quantity in `chem` and `physics` does too. Whether that is
   `Uncertain of T` propagating first-order error, or out of scope, is not
   decided here and should be — it interacts with `Quantity` and it is much
   cheaper to design them together than to add one to the other later.

---

## 15. Risks

**The catalogue is a commitment surface.** Eight modules with this many names is
several hundred person-months. Publishing the catalogue without publishing the
sequencing invites the reading that all of it is planned; §13 is the antidote and
should travel with it.

**Linking is a distribution problem, not a technical one.** §2 decides to link
BLAS, LAPACK, FFTW and SuiteSparse. `ffi-c-boundary.md` §5.1 handles finding
them; nothing handles *shipping* them. A scientific language whose `linalg`
requires the user to install OpenBLAS first will be judged on that first
experience. The package manager is out of scope for both notes and this is the
seam where its absence will hurt most.

**`stats` and `data` overlap at the `Frame`.** §7.6 puts table operations in
`stats.frame` over `data-io.md`'s `Frame of R`. If that split turns out wrong,
the fix is a module move and not a redesign — but it should be revisited once
`Frame` exists, rather than inherited from this note by default.
