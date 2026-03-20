;; Review pr: view details then diff then checks
(define (review-pr (dir : String) (pr_number : String))
  (bind dir ".")
  (bind pr_number "1")
  (gh_pr_view)
  (gh_pr_diff :dir "$dir" :pr_number "$pr_number")
  (gh_pr_checks :dir "$dir" :pr_number "$pr_number"))
