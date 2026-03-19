;; Check env variable value
(define (env-check (var_name : String))
  (bind var_name "HOME")
  (env_get))
