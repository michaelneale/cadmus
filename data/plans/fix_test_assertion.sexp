;; Fix assertion value in a test then run tests
(define (fix-test-assertion (dir : String) (file : String) (old_value : String) (new_value : String))
  (bind dir ".")
  (bind file "tests/test.rs")
  (bind old_value "42")
  (bind new_value "43")
  (fix_assertion)
  (test_project :dir "$dir"))
