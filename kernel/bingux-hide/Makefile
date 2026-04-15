obj-m += bingux_hide.o

KDIR ?= /lib/modules/$(shell uname -r)/build

all:
	$(MAKE) -C $(KDIR) M=$(PWD) modules

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean

install:
	cp bingux_hide.ko /system/packages/bingux-hide-1.0.0-x86_64-linux/
