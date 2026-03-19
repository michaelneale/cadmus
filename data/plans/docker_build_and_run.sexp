;; Build a Docker image and run it
(define (docker-build-and-run (dir : String) (tag : String))
  (bind dir ".")
  (bind tag "app")
  (docker_build)
  (docker_run :image "$tag" :args ""))
