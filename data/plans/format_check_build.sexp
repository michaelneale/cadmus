;; Format, type-check, then build a project
(define (format-check-build (dir : String))
  (bind dir ".")
  (format_project)
  (check_project :dir "$dir")
  (build_project :dir "$dir"))
