; Program included in lc3.rgate. No assembler is bundled; lc3-program.hex
; contains the corresponding words. All addresses are word addresses.
        .ORIG x3000
        AND R0, R0, #0
        AND R1, R1, #0
        ADD R1, R1, #5
LOOP    ADD R0, R0, R1
        ADD R1, R1, #-1
        BRp LOOP
        ST R0, RESULT       ; sum(1..5) = 15
        LD R0, LETTER_H
        TRAP x21
        LD R0, LETTER_I
        TRAP x21
        LD R0, NEWLINE
        TRAP x21
        TRAP x25            ; microcode halts for this vector
        .END

        .ORIG x3020
RESULT  .FILL #0
LETTER_H .FILL x0048
LETTER_I .FILL x0049
NEWLINE .FILL x000A
        .END

; OUT is ordinary LC-3 code reached through trap-vector memory.
        .ORIG x0021
        .FILL x3100
        .END

        .ORIG x3100
OUT     LD R2, DDR
        STR R0, R2, #0      ; circuit decodes xFE06 to strobe the TTY
        RET
DDR     .FILL xFE06
        .END
; This minimal handler clobbers R2. It is not a complete LC-3 OS.
