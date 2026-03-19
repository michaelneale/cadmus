;; Deploy fly app and check status
(define (fly-deploy-and-check (dir : String) (app_name : String))
  (bind dir ".")
  (bind app_name "myapp")
  (fly_deploy)
  (fly_status :app_name "$app_name"))
