;; List available just recipes in a project
(define (just-recipes (dir : String))
  (bind dir ".")
  (just_list))
