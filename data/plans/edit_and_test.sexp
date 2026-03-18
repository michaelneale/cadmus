;; Sed replace a pattern in a file then test the project
(define (edit-and-test (dir : String) (file : String) (find : String) (replace : String))
  (bind dir ".")
  (bind file "src/main.rs")
  (bind find "old_value")
  (bind replace "new_value")
  (sed_replace)
  (test_project :dir "$dir"))
