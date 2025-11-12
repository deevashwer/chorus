#include "interface.h"
#include "bicycl.hpp"

extern "C" {
void QFI_delete(QFI* x) {
    BICYCL::QFI* x_ = reinterpret_cast<BICYCL::QFI*>(x);
    delete x_;
}

void CL_HSMqk_delete(CL_HSMqk* C) {
    BICYCL::CL_HSMqk* C_ = reinterpret_cast<BICYCL::CL_HSMqk*>(C);
    delete C_;
}

CL_HSMqk* CL_HSMqk_init(const mpz_t* q, const mpz_t* seed, unsigned int seclevel) {
    BICYCL::SecLevel seclevel_ = BICYCL::SecLevel(seclevel);
    BICYCL::RandGen randgen;
    const BICYCL::Mpz* seed_ = reinterpret_cast<const BICYCL::Mpz*>(seed);
    const BICYCL::Mpz* q_ = reinterpret_cast<const BICYCL::Mpz*>(q);
    randgen.set_seed (*seed_);
    BICYCL::CL_HSMqk* C = new BICYCL::CL_HSMqk(*q_, 1, seclevel_, randgen, false);
    CL_HSMqk* C_ = reinterpret_cast<CL_HSMqk*>(C);
    return C_;
}

QFI* CL_HSMqk_power_of_h (const CL_HSMqk* C, const mpz_t* e) {
    const BICYCL::CL_HSMqk* C_ = reinterpret_cast<const BICYCL::CL_HSMqk*>(C);
    const BICYCL::Mpz* e_ = reinterpret_cast<const BICYCL::Mpz*>(e);
    BICYCL::QFI* r = new BICYCL::QFI();
    C_->power_of_h(*r, *e_);
    QFI* r_ = reinterpret_cast<QFI*>(r);
    return r_;
}

QFI* CL_HSMqk_power_of_f (const CL_HSMqk* C, const mpz_t* m) {
    const BICYCL::CL_HSMqk* C_ = reinterpret_cast<const BICYCL::CL_HSMqk*>(C);
    const BICYCL::Mpz* m_ = reinterpret_cast<const BICYCL::Mpz*>(m);
    BICYCL::QFI* f_m = new BICYCL::QFI();
    *f_m = C_->power_of_f(*m_);
    QFI* f_m_ = reinterpret_cast<QFI*>(f_m);
    return f_m_;
}

mpz_t* CL_HSMqk_dlog_in_F (const CL_HSMqk* C, const QFI* fm) {
    const BICYCL::CL_HSMqk* C_ = reinterpret_cast<const BICYCL::CL_HSMqk*>(C);
    const BICYCL::QFI* fm_ = reinterpret_cast<const BICYCL::QFI*>(fm);
    BICYCL::Mpz* m = new BICYCL::Mpz();
    *m = C_->dlog_in_F(*fm_);
    mpz_t* m_ = reinterpret_cast<mpz_t*>(m);
    return m_;
}



QFI* ClassGroup_one (const ClassGroup* G) {
    const BICYCL::ClassGroup* G_ = reinterpret_cast<const BICYCL::ClassGroup*>(G);
    BICYCL::QFI* r = new BICYCL::QFI();
    *r = G_->one();
    QFI* r_ = reinterpret_cast<QFI*>(r);
    return r_;
}

QFI* ClassGroup_nucomp (const ClassGroup* G, const QFI* f1, const QFI* f2) {
    const BICYCL::ClassGroup* G_ = reinterpret_cast<const BICYCL::ClassGroup*>(G);
    const BICYCL::QFI* f1_ = reinterpret_cast<const BICYCL::QFI*>(f1);
    const BICYCL::QFI* f2_ = reinterpret_cast<const BICYCL::QFI*>(f2);
    BICYCL::QFI* r = new BICYCL::QFI();
    G_->nucomp(*r, *f1_, *f2_);
    QFI* r_ = reinterpret_cast<QFI*>(r);
    return r_;
}

QFI* ClassGroup_nucompinv (const ClassGroup* G, const QFI* f1, const QFI* f2) {
    const BICYCL::ClassGroup* G_ = reinterpret_cast<const BICYCL::ClassGroup*>(G);
    const BICYCL::QFI* f1_ = reinterpret_cast<const BICYCL::QFI*>(f1);
    const BICYCL::QFI* f2_ = reinterpret_cast<const BICYCL::QFI*>(f2);
    BICYCL::QFI* r = new BICYCL::QFI();
    G_->nucompinv(*r, *f1_, *f2_);
    QFI* r_ = reinterpret_cast<QFI*>(r);
    return r_;
}

QFI* ClassGroup_nudupl (const ClassGroup* G, const QFI* f) {
    const BICYCL::ClassGroup* G_ = reinterpret_cast<const BICYCL::ClassGroup*>(G);
    const BICYCL::QFI* f_ = reinterpret_cast<const BICYCL::QFI*>(f);
    BICYCL::QFI* r = new BICYCL::QFI();
    G_->nudupl(*r, *f_);
    QFI* r_ = reinterpret_cast<QFI*>(r);
    return r_;
}

QFI* ClassGroup_nudupl_n (const ClassGroup* G, const QFI* f, size_t niter) {
    const BICYCL::ClassGroup* G_ = reinterpret_cast<const BICYCL::ClassGroup*>(G);
    const BICYCL::QFI* f_ = reinterpret_cast<const BICYCL::QFI*>(f);
    BICYCL::QFI* r = new BICYCL::QFI();
    G_->nudupl(*r, *f_, niter);
    QFI* r_ = reinterpret_cast<QFI*>(r);
    return r_;
}

QFI* ClassGroup_nupow (const ClassGroup* G, const QFI* f, const mpz_t* n) {
    const BICYCL::ClassGroup* G_ = reinterpret_cast<const BICYCL::ClassGroup*>(G);
    const BICYCL::QFI* f_ = reinterpret_cast<const BICYCL::QFI*>(f);
    const BICYCL::Mpz* n_ = reinterpret_cast<const BICYCL::Mpz*>(n);
    BICYCL::QFI* r = new BICYCL::QFI();
    G_->nupow(*r, *f_, *n_);
    QFI* r_ = reinterpret_cast<QFI*>(r);
    return r_;
}

QFI* ClassGroup_nupow_wprecomp (const ClassGroup* G, const QFI* f, const mpz_t* n, size_t d, size_t e, const QFI* fe, const QFI* fd, const QFI* fed) {
    const BICYCL::ClassGroup* G_ = reinterpret_cast<const BICYCL::ClassGroup*>(G);
    const BICYCL::QFI* f_ = reinterpret_cast<const BICYCL::QFI*>(f);
    const BICYCL::QFI* fe_ = reinterpret_cast<const BICYCL::QFI*>(fe);
    const BICYCL::QFI* fd_ = reinterpret_cast<const BICYCL::QFI*>(fd);
    const BICYCL::QFI* fed_ = reinterpret_cast<const BICYCL::QFI*>(fed);
    const BICYCL::Mpz* n_ = reinterpret_cast<const BICYCL::Mpz*>(n);
    BICYCL::QFI* r = new BICYCL::QFI();
    G_->nupow(*r, *f_, *n_, d, e, *fe_, *fd_, *fed_);
    QFI* r_ = reinterpret_cast<QFI*>(r);
    return r_;
}
}
