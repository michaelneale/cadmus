;; Create release with a tag
(define (create-release (dir : String) (tag : String) (notes : String))
  (bind dir ".")
  (bind tag "v1.0.0")
  (bind notes "Release")
  (gh_release_create))
