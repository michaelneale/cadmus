;; Type-check then lint a project
(define (check-and-lint (dir : String))
  (bind dir ".")
  (check_project)
  (lint_project :dir "$dir"))
