obj-m += bingux_hide.o

KDIR ?= /lib/modules/$(shell uname -r)/build
MOD_DIR := $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))

all:
	$(MAKE) -C $(KDIR) M=$(MOD_DIR) modules

clean:
	$(MAKE) -C $(KDIR) M=$(MOD_DIR) clean

install:
	cp bingux_hide.ko /system/packages/bingux-hide-1.0.0-x86_64-linux/
