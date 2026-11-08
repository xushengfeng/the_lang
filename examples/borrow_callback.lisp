; A closure may capture a borrow only while it stays inside the caller.
; Same program as examples/borrow_callback.alt.

(fn first ((borrow a (array Int))) Int
  (let reader (lambda ((i Int)) Int (index a i))
    (reader 0)))

(fn main () Int
  (let a (array 4 5 6)
    (first a)))
