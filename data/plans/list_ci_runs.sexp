;; List ci runs for the repo
(define (list-ci-runs (dir : String))
  (bind dir ".")
  (gh_run_list))
