;; List releases for the repo
(define (list-releases (dir : String))
  (bind dir ".")
  (gh_release_list))
