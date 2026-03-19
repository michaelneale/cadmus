;; List open pull requests for the repo
(define (list-prs (dir : String))
  (bind dir ".")
  (gh_pr_list))
