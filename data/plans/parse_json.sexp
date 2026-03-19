;; Parse a JSON file with a jq query
(define (parse-json (file : String) (query : String))
  (bind file "data.json")
  (bind query ".")
  (jq_parse))
