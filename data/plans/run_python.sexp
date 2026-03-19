;; Run a Python script
(define (run-python (script : String))
  (bind script "main.py")
  (run_python))
