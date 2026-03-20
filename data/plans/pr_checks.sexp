;; CI checks status for a pr
(define (pr-checks (dir : String) (pr_number : String))
  (bind dir ".")
  (bind pr_number "1")
  (gh_pr_checks))
