#include <gmp.h>
#include <stdbool.h>

typedef struct QFI {
    mpz_t a_, b_, c_;
} QFI;

typedef struct ClassGroup {
      mpz_t disc_;
      mpz_t default_nucomp_bound_;
      mpz_t class_number_bound_;
} ClassGroup;

typedef struct CL_HSMqk {
      mpz_t q_;
      size_t k_;
      mpz_t p_;
      mpz_t M_;
      ClassGroup Cl_DeltaK_;
      ClassGroup Cl_Delta_;
      bool compact_variant_;
      bool large_message_variant_;
      QFI h_;
      unsigned int distance_;
      mpz_t exponent_bound_;
      size_t d_;
      size_t e_;
      QFI h_e_precomp_;
      QFI h_d_precomp_;
      QFI h_de_precomp_;
} CL_HSMqk;

#ifdef __cplusplus
extern "C" {
#endif
    void QFI_delete(QFI* x);
    void CL_HSMqk_delete(CL_HSMqk* C);

    CL_HSMqk* CL_HSMqk_init(const mpz_t* q, const mpz_t* seed, unsigned int seclevel);
    // Set r to h^e, where h is the generator of H
    QFI* CL_HSMqk_power_of_h (const CL_HSMqk* C, const mpz_t* e);
    // Return f^m, where `f` is the generator of F.
    QFI* CL_HSMqk_power_of_f (const CL_HSMqk* C, const mpz_t* m);
    // Return the discrete logarithm of fm with base f.
    mpz_t* CL_HSMqk_dlog_in_F (const CL_HSMqk* C, const QFI* fm);

    // return identity element
    QFI* ClassGroup_one (const ClassGroup* G);
    // f1 + f2
    QFI* ClassGroup_nucomp (const ClassGroup* G, const QFI* f1, const QFI* f2);
    // f1 - f2
    QFI* ClassGroup_nucompinv (const ClassGroup* G, const QFI* f1, const QFI* f2);
    // 2 * f
    QFI* ClassGroup_nudupl (const ClassGroup* G, const QFI* f);
    // 2^niter * f
    QFI* ClassGroup_nudupl_n (const ClassGroup* G, const QFI* f, size_t niter);
    // f^n
    QFI* ClassGroup_nupow (const ClassGroup* G, const QFI* f, const mpz_t* n);
    // f^n with some precomputation on f
    QFI* ClassGroup_nupow_wprecomp (const ClassGroup* G, const QFI* f, const mpz_t* n, size_t d, size_t e, const QFI* fe, const QFI* fd, const QFI* fed);
#ifdef __cplusplus
}
#endif