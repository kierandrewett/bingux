obj-m += bingux_compat.o

KDIR ?= /lib/modules/$(shell uname -r)/build
MOD_DIR := $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))

all:
	$(MAKE) -C $(KDIR) M=$(MOD_DIR) modules

clean:
	$(MAKE) -C $(KDIR) M=$(MOD_DIR) clean
