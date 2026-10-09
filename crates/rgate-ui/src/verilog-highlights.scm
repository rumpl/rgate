(comment) @comment
(simple_identifier) @variable
(system_tf_identifier) @function
[(integral_number) (real_number)] @number
(string_literal) @string
[(interface_identifier) (class_identifier)] @type
[(function_identifier) (task_identifier)] @function
["module" "endmodule" "input" "output" "inout" "wire" "reg" "logic"
 "assign" "always" "always_ff" "always_comb" "always_latch" "initial" "final"
 "begin" "end" "if" "else" "case" "endcase" "for" "while" "repeat"
 "parameter" "localparam" "generate" "endgenerate" "posedge" "negedge"
 "function" "endfunction" "task" "endtask"] @keyword
["+" "-" "*" "/" "=" "<=" ">=" "<" ">" "==" "!=" "&" "|" "^" "~" "!"] @operator
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
[";" "," ":" "."] @punctuation.delimiter
