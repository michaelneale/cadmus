;; Fix import path in a source file
(define (fix-import (file : String) (old_path : String) (new_path : String))
  (bind file "src/lib.rs")
  (bind old_path "super::")
  (bind new_path "crate::")
  (fix_import))
