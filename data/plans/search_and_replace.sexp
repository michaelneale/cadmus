;; Grep for a code pattern then sed replace it
(define (search-and-replace (dir : String) (pattern : String) (replacement : String))
  (bind dir ".")
  (bind pattern "TODO")
  (bind replacement "DONE")
  (grep_code)
  (sed_replace :file "$dir" :find "$pattern" :replace "$replacement"))
