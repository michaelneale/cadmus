;; Run a just recipe
(define (run-recipe (dir : String) (recipe : String))
  (bind dir ".")
  (bind recipe "build")
  (just_run))
