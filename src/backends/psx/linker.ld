/* SPDX-License-Identifier: GPL-3.0-or-later
 * Copyright (C) 2026 Renan Lucas Vieira Hilário
 *
 * PlayStation 1 link script: load at 0x80010000, define _gp and the .bss
 * bounds (cleared by the naked startup), discard DWARF/unwind/host sections.
 */

OUTPUT_ARCH(mips)
ENTRY(_start)

SECTIONS
{
    . = 0x80010000;

    .text : {
        *(.text)
        *(.text.*)
    }

    .rodata : {
        *(.rodata)
        *(.rodata.*)
    }

    .data : {
        *(.data)
        *(.data.*)
    }

    .sdata : ALIGN(16) {
        _gp = . + 0x7ff0;
        *(.sdata)
        *(.sdata.*)
    }

    .bss : ALIGN(4) {
        _bssStart = .;
        *(.bss)
        *(.bss.*)
        *(.sbss)
        *(.sbss.*)
        *(COMMON)
        . = ALIGN(4);
        _bssEnd = .;
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.eh_frame.*)
        *(.rel.*)
        *(.rela.*)
        *(.got)
        *(.got.*)
        *(.MIPS.abiflags)
        *(.reginfo)
        *(.pdr)
        *(.comment)
        *(.note*)
        *(.mdebug*)
    }
}
