;; Run node script
(define (run-node (script : String))
  (bind script "index.js")
  (run_node))
