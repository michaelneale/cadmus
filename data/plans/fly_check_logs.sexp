;; Check fly logs for an app
(define (fly-check-logs (app_name : String))
  (bind app_name "myapp")
  (fly_logs))
