;; Add after a pattern in a file then build project
(define (add-wiring-line (dir : String) (file : String) (after_pattern : String) (new_line : String))
  (bind dir ".")
  (bind file "src/lib.rs")
  (bind after_pattern "mod existing")
  (bind new_line "mod new_module;")
  (add_after)
  (build_project :dir "$dir"))
