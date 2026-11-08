; Kernel-checked mathematical proofs, written as explicit terms.
; Run: cargo run --bin proof_demo -- examples/proofs.lisp

; ∀ p : Prop, p -> p
(proof imp_refl
  (forall ((p Prop)) (forall ((h p)) p))
  (lambda ((p Prop)) (lambda ((h p)) h)))

; ∀ n : Nat, n = n
(proof nat_refl
  (forall ((n Nat)) (Eq Nat n n))
  (lambda ((n Nat)) (Eq.refl Nat n)))

; 2 + 2 = 4, by computation and reflexivity
(proof two_add_two
  (Eq Nat (Nat.add 2 2) 4)
  (Eq.refl Nat 4))

; A proof using an earlier theorem as a constant.
(proof imp_refl_again
  (forall ((p Prop)) (forall ((h p)) p))
  imp_refl)

; Induction: ∀ n : Nat, n + 0 = n, by Nat.recProp.
(proof add_zero_surface
  (forall ((n Nat)) (Eq Nat (Nat.add n 0) n))
  (lambda ((n Nat))
    (Nat.recProp
      (lambda ((n Nat)) (Eq Nat (Nat.add n 0) n))
      (Eq.refl Nat 0)
      (lambda ((n Nat))
        (lambda ((ih (Eq Nat (Nat.add n 0) n)))
          (congr_succ (Nat.add n 0) n ih)))
      n)))
