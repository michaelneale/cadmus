;; Deploy app to hosting platform
(define (deploy (dir : String))
  (bind dir ".")
  (fly_deploy))
