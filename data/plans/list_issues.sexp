;; List open GitHub issues for the repo
(define (list-issues (dir : String))
  (bind dir ".")
  (gh_issue_list))
