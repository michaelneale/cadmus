;; Replace a pattern in a source file then build the project
(define (edit-and-build (dir : String) (file : String) (find : String) (replace : String))
  (bind dir ".")
  (bind file "src/main.rs")
  (bind find "old_name")
  (bind replace "new_name")
  (sed_replace)
  (build_project :dir "$dir"))
