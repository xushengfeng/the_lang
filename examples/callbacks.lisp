; Callbacks and higher-order functions, s-expression frontend.
; This lowers to the same AST as examples/callbacks.alt.

(fn apply_twice ((f (fn (Int) Int)) (x Int)) Int
  (f (f x)))

(fn add_one ((x Int)) Int
  (+ x 1))

(fn make_adder ((n Int)) (fn (Int) Int)
  (lambda ((y Int)) Int (+ y n)))

(fn main () Int
  (let double (lambda ((y Int)) Int (* y 2))
    (let a (apply_twice double 5)
      (let b (apply_twice add_one 1)
        (let add10 (make_adder 10)
          (let c (add10 7)
            (let d ((make_adder 3) 4)
              (seq
                (print a)
                (print b)
                (print c)
                (print d)
                (+ (+ (+ a b) c) d)))))))))
