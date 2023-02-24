# x86_ATA

----
All credit goes to NPEX42, I made this since the operating system I've been working on breaks \nWhen the x86_64 crate is imported and this removes it. A Simple, Amazing x86 ATA Crate. Credit to NPEX42
## Overview

- 24-bit LBA mode
- Uses PIO Mode

## Examples

```rust
// Read A Single block from a disk
pub fn read_single() {
    use ata_x86::{init, ATA_BLOCK_SIZE, read};
    // 1. Initialise ATA Subsystem. (Perform Once, on boot)
    init().expect("Failed To Start ATA...");
    
    // 2. Create a temporary buffer of size 512.
    let mut buffer: [u8;ATA_BLOCK_SIZE] = [0; ATA_BLOCK_SIZE];

    // 3. Pass the buffer over to the Subsystem, to be filled.
    read(0, 0, 0, &mut buffer);
}


// Write A Single block onto a disk
pub fn write_single() {
    use ata_x86::{init, ATA_BLOCK_SIZE, write};
    // 1. Initialise ATA Subsystem. (Perform Once, on boot)
    init().expect("Failed To Start ATA...");
    
    // 2. Create a buffer of size 512, filled with the data to be written.
    let buffer: [u8;ATA_BLOCK_SIZE] = [0; ATA_BLOCK_SIZE];

    // 3. Pass the buffer over to the Subsystem, to be filled.
    write(0, 0, 0, &buffer);
}
```
