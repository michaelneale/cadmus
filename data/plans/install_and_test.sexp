;; Install dependencies then run tests
(define (install-and-test (dir : String))
  (bind dir ".")
  (install_deps)
  (test_project :dir "$dir"))
