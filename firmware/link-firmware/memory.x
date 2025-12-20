MEMORY
{
  /*
	The flash area is divided into 9 segments:
	- 2 64K segments for the boot information
	- 7 128K segments for the main firmware and data storage

	Erasure on NOR memory is only possible on entire segments at a time,
	so the persistent flash settings need to be aligned to the last segment.
  */

  CCMRAM   (xrw)    : ORIGIN = 0x10000000,    LENGTH = 64K
  RAM      (xrw)    : ORIGIN = 0x20000000,    LENGTH = 320K
  BKPSRAM  (xrw)    : ORIGIN = 0x40024000,    LENGTH = 4K
  FLASH    (rx)     : ORIGIN = 0x08000000,    LENGTH = (64K + 64K + (128K * 6))
}

_stack_start = ORIGIN(RAM) + LENGTH(RAM);
_persistent_flash_start = ORIGIN(FLASH) + LENGTH(FLASH);
_persistent_flash_end = ORIGIN(FLASH) + 1024K;

/* We make sure that the persistent flash area is aligned to segment boundaries... */
ASSERT((_persistent_flash_start % 0x20000) == 0, "Persistent flash start is not segment-aligned");
/* ... and that it is 128K in size */
ASSERT((_persistent_flash_end - _persistent_flash_start) % 0x20000 == 0, "Persistent flash size is not a multiple of segment size");

SECTIONS
{
	.bkpsram (NOLOAD) :
	{
		KEEP(*(.bkpsram))
	} > BKPSRAM
}
