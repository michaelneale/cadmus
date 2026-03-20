;; Rename symbol then build to verify
(define (rename-and-build (dir : String) (old_name : String) (new_name : String))
  (bind dir ".")
  (bind old_name "old")
  (bind new_name "new")
  (rename_symbol)
  (build_project :dir "$dir"))
