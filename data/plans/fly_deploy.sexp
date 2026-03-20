;; Fly deploy an app
(define (fly-deploy (dir : String))
  (bind dir ".")
  (fly_deploy))
