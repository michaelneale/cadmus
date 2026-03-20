;; Grep fix build: search for a pattern, replace it, then build
(define (grep-fix-build (dir : String) (pattern : String) (replacement : String))
  (bind dir ".")
  (bind pattern "old_name")
  (bind replacement "new_name")
  (grep_code)
  (sed_replace :file "$dir" :find "$pattern" :replace "$replacement")
  (build_project :dir "$dir"))
