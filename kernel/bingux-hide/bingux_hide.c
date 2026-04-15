/*
 * bingux_hide — Kernel module to hide FHS compatibility symlinks from ls /
 *
 * Bingux uses /system, /users, /run as its root layout.
 * FHS symlinks (/bin, /lib, /lib64, /usr, /sbin, /opt, /var, /home)
 * exist for compatibility but should be invisible to casual listing.
 *
 * This module hooks getdents64() and filters out entries whose names
 * match the hidden list when listing the root directory (/).
 *
 * The hidden entries still work — programs can open /bin/sh, /usr/lib,
 * etc. They just won't appear in ls / or readdir(/).
 */

#include <linux/module.h>
#include <linux/kernel.h>
#include <linux/kprobes.h>
#include <linux/dirent.h>
#include <linux/uaccess.h>
#include <linux/slab.h>
#include <linux/fs.h>
#include <linux/file.h>
#include <linux/fdtable.h>
#include <linux/dcache.h>
#include <linux/namei.h>
#include <linux/string.h>
#include <linux/version.h>

MODULE_LICENSE("GPL");
MODULE_AUTHOR("Bingux Project");
MODULE_DESCRIPTION("Hide FHS compatibility dirs from ls /");
MODULE_VERSION("1.0");

/* Directories to hide from root listing */
static const char *hidden_names[] = {
	"bin", "sbin", "lib", "lib64", "usr", "opt",
	"home", "var", "mnt", "media", "srv",
	NULL
};

/* Module parameter: enable/disable at runtime */
static bool enabled = true;
module_param(enabled, bool, 0644);
MODULE_PARM_DESC(enabled, "Enable directory hiding (default: true)");

/*
 * Check if a name should be hidden.
 * Only hides entries that are symlinks in the root directory.
 */
static bool should_hide(const char *name)
{
	const char **p;
	if (!enabled)
		return false;
	for (p = hidden_names; *p; p++) {
		if (strcmp(name, *p) == 0)
			return true;
	}
	return false;
}

/*
 * kretprobe handler for __x64_sys_getdents64.
 *
 * After the syscall returns, we walk the userspace buffer and
 * remove entries that match our hidden list — but ONLY when
 * the fd refers to the root directory.
 */

/* Per-instance data to carry fd across entry/return */
struct getdents_data {
	int fd;
	unsigned long dirp;
};

static int entry_handler(struct kretprobe_instance *ri, struct pt_regs *regs)
{
	struct getdents_data *data = (struct getdents_data *)ri->data;
	/*
	 * On x86_64 with syscall wrappers, the __x64_sys_* function
	 * receives a pointer to the original pt_regs in rdi.
	 * The actual syscall args are in that pointed-to struct.
	 */
	struct pt_regs *uregs = (struct pt_regs *)regs->di;
	data->fd = (int)uregs->di;
	data->dirp = uregs->si;
	return 0;
}

/*
 * Check if an fd points to the root directory.
 */
static bool fd_is_root(int fd)
{
	struct file *f;
	struct dentry *dentry;
	bool is_root = false;

	f = fget(fd);
	if (!f)
		return false;

	dentry = f->f_path.dentry;
	/* Root directory: parent == self */
	if (dentry && IS_ROOT(dentry)) {
		is_root = true;
	}
	fput(f);
	return is_root;
}

static int return_handler(struct kretprobe_instance *ri, struct pt_regs *regs)
{
	struct getdents_data *data = (struct getdents_data *)ri->data;
	long ret = regs_return_value(regs);
	struct linux_dirent64 __user *dirp;
	struct linux_dirent64 *kbuf, *src, *dst;
	long remaining, new_len;
	unsigned short reclen;

	if (ret <= 0 || !enabled)
		return 0;

	/* Only filter root directory listings */
	if (!fd_is_root(data->fd))
		return 0;

	dirp = (struct linux_dirent64 __user *)data->dirp;

	/* Copy the dirent buffer to kernel space */
	kbuf = kmalloc(ret, GFP_KERNEL);
	if (!kbuf)
		return 0;

	if (copy_from_user(kbuf, dirp, ret)) {
		kfree(kbuf);
		return 0;
	}

	/* Walk entries: copy non-hidden ones to the front */
	src = kbuf;
	dst = kbuf;
	remaining = ret;
	new_len = 0;

	while (remaining > 0) {
		reclen = src->d_reclen;
		if (reclen == 0 || reclen > remaining)
			break;

		if (!should_hide(src->d_name)) {
			if (dst != src)
				memmove(dst, src, reclen);
			dst = (struct linux_dirent64 *)((char *)dst + reclen);
			new_len += reclen;
		}

		src = (struct linux_dirent64 *)((char *)src + reclen);
		remaining -= reclen;
	}

	/* Copy filtered buffer back to userspace */
	if (new_len != ret) {
		if (copy_to_user(dirp, kbuf, new_len)) {
			kfree(kbuf);
			return 0;
		}
		/* Update return value to reflect new buffer size */
		regs_set_return_value(regs, new_len);
	}

	kfree(kbuf);
	return 0;
}

static struct kretprobe krp64 = {
	.handler = return_handler,
	.entry_handler = entry_handler,
	.data_size = sizeof(struct getdents_data),
	.maxactive = 20,
};

static struct kretprobe krp = {
	.handler = return_handler,
	.entry_handler = entry_handler,
	.data_size = sizeof(struct getdents_data),
	.maxactive = 20,
};

static int __init bingux_hide_init(void)
{
	int ret;

	/* Hook both getdents and getdents64 */
	krp64.kp.symbol_name = "__x64_sys_getdents64";
	ret = register_kretprobe(&krp64);
	if (ret < 0)
		pr_warn("bingux_hide: getdents64 probe failed: %d\n", ret);
	else
		pr_info("bingux_hide: hooked getdents64\n");

	krp.kp.symbol_name = "__x64_sys_getdents";
	ret = register_kretprobe(&krp);
	if (ret < 0)
		pr_warn("bingux_hide: getdents probe failed: %d\n", ret);
	else
		pr_info("bingux_hide: hooked getdents\n");

	pr_info("bingux_hide: loaded — hiding FHS dirs from ls /\n");
	return 0;
}

static void __exit bingux_hide_exit(void)
{
	unregister_kretprobe(&krp64);
	unregister_kretprobe(&krp);
	pr_info("bingux_hide: unloaded — FHS dirs visible again\n");
}

module_init(bingux_hide_init);
module_exit(bingux_hide_exit);
