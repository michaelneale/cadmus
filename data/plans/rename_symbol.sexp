;; Rename symbol across codebase
(define (rename-symbol (dir : String) (old_name : String) (new_name : String))
  (bind dir ".")
  (bind old_name "old")
  (bind new_name "new")
  (rename_symbol))
