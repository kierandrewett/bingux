/*
 * bingux_compat — Kernel module for transparent FHS path translation
 *
 * Bingux uses a three-directory root: /system, /users, /io
 * No other entries exist at /.  FHS compatibility is achieved by:
 *
 *   1. Overriding the root inode's lookup to create virtual symlinks
 *      for FHS paths (/bin, /usr, /etc, etc.)
 *   2. Filtering the root directory listing (readdir) to hide bind
 *      mount points (/dev, /proc, /sys) and boot artifacts (/init)
 *
 * Result:
 *   ls /           → system  users  io    (the only visible entries)
 *   ls /etc        → works   (resolves /system/state/ephemeral/etc)
 *   cat /bin/sh    → works   (resolves /system/profiles/current/bin/sh)
 *   ls /dev/dri    → works   (bind mount from /io)
 *   cat /proc/1/status → works (bind mount from /system/kernel/proc)
 *
 * On unload, all original operations are restored.
 */

#include <linux/module.h>
#include <linux/kernel.h>
#include <linux/fs.h>
#include <linux/dcache.h>
#include <linux/namei.h>
#include <linux/string.h>
#include <linux/slab.h>
#include <linux/version.h>

MODULE_LICENSE("GPL");
MODULE_AUTHOR("Bingux Project");
MODULE_DESCRIPTION("Transparent FHS path translation for Bingux root layout");
MODULE_VERSION("1.1");

/* ── FHS path mapping table ─────────────────────────────────────────── */

struct compat_mapping {
	const char *fhs_name;
	const char *bingux_path;
};

static const struct compat_mapping mappings[] = {
	/* FHS binary/library dirs → current system profile */
	{ "bin",    "/system/profiles/current/bin"   },
	{ "sbin",   "/system/profiles/current/sbin"  },
	{ "lib",    "/system/profiles/current/lib"   },
	{ "lib64",  "/system/profiles/current/lib64" },
	{ "usr",    "/system/profiles/current/usr"   },
	{ "opt",    "/system/profiles/current/opt"   },

	/* System configuration (generated at boot on tmpfs) */
	{ "etc",    "/system/state/ephemeral/etc"    },

	/* Runtime / volatile state */
	{ "run",    "/system/state/ephemeral/run"    },
	{ "var",    "/system/state/ephemeral/var"    },
	{ "tmp",    "/system/tmp"                    },

	/* User directories */
	{ "home",   "/users"                         },
	{ "root",   "/users/root"                    },

	/* Boot */
	{ "boot",   "/system/boot"                   },

	/* Misc FHS dirs */
	{ "mnt",    "/system/state/ephemeral/mnt"    },
	{ "media",  "/system/state/ephemeral/media"  },
	{ "srv",    "/system/state/ephemeral/srv"    },

	{ NULL, NULL }
};

/*
 * Entries to hide from ls / (readdir filter).
 *
 * These are real directories/files on the rootfs that exist for
 * technical reasons but should not be visible to the user:
 *   - dev, proc, sys: bind mount points (needed by programs that
 *     call realpath and expect /dev/..., /proc/..., /sys/...)
 *   - init: the boot script (initramfs artifact)
 */
static const char *hidden_from_listing[] = {
	"dev", "proc", "sys", "init", "root", NULL
};

/* ── Saved original operations ──────────────────────────────────────── */

static const struct inode_operations *orig_root_i_op;
static struct inode_operations compat_root_i_op;
static const struct file_operations *orig_root_f_op;
static struct file_operations compat_root_f_op;

/* Inode operations for virtual symlink inodes */
static struct inode_operations compat_symlink_i_op;

/* ── Helpers ────────────────────────────────────────────────────────── */

static const struct compat_mapping *find_mapping(const char *name)
{
	const struct compat_mapping *m;
	for (m = mappings; m->fhs_name; m++) {
		if (strcmp(name, m->fhs_name) == 0)
			return m;
	}
	return NULL;
}

static bool should_hide_from_listing(const char *name)
{
	const char **p;
	for (p = hidden_from_listing; *p; p++) {
		if (strcmp(name, *p) == 0)
			return true;
	}
	return false;
}

/* ── Virtual symlink inode ──────────────────────────────────────────── */

static const char *compat_get_link(struct dentry *dentry, struct inode *inode,
				   struct delayed_call *done)
{
	if (!inode)
		return ERR_PTR(-ECHILD);
	return inode->i_link;
}

static struct inode *make_virtual_symlink(struct super_block *sb,
					  const char *target)
{
	struct inode *inode;

	inode = new_inode(sb);
	if (!inode)
		return NULL;

	inode->i_ino = get_next_ino();
	inode->i_mode = S_IFLNK | 0777;
	inode->i_uid = GLOBAL_ROOT_UID;
	inode->i_gid = GLOBAL_ROOT_GID;
	inode->i_op = &compat_symlink_i_op;
	inode->i_link = (char *)target;
	inode->i_size = strlen(target);

	return inode;
}

/* ── Root inode lookup override ─────────────────────────────────────── */

static struct dentry *compat_lookup(struct inode *dir, struct dentry *dentry,
				    unsigned int flags)
{
	const struct compat_mapping *m;
	struct inode *inode;

	m = find_mapping(dentry->d_name.name);
	if (!m) {
		if (orig_root_i_op && orig_root_i_op->lookup)
			return orig_root_i_op->lookup(dir, dentry, flags);
		d_add(dentry, NULL);
		return NULL;
	}

	inode = make_virtual_symlink(dir->i_sb, m->bingux_path);
	if (!inode) {
		d_add(dentry, NULL);
		return NULL;
	}

	d_add(dentry, inode);
	return NULL;
}

/* ── Root directory listing filter (readdir) ────────────────────────── */

/*
 * Wrapper context for filtering readdir output.
 * We intercept each entry from the real iterate_shared and only
 * pass through entries that should be visible.
 */
struct filter_ctx {
	struct dir_context ctx;        /* must be first */
	struct dir_context *orig_ctx;  /* the caller's real context */
};

static bool filter_actor(struct dir_context *ctx, const char *name, int namlen,
			 loff_t offset, u64 ino, unsigned int d_type)
{
	struct filter_ctx *fctx = container_of(ctx, struct filter_ctx, ctx);
	char buf[256];

	/* Safety: don't overflow our stack buffer */
	if (namlen >= sizeof(buf))
		return fctx->orig_ctx->actor(fctx->orig_ctx, name, namlen,
					     offset, ino, d_type);

	memcpy(buf, name, namlen);
	buf[namlen] = '\0';

	/* Filter: skip entries that should be hidden */
	if (should_hide_from_listing(buf))
		return true;  /* true = continue iterating, just skip this one */

	/* Pass through to the real caller */
	return fctx->orig_ctx->actor(fctx->orig_ctx, name, namlen,
				     offset, ino, d_type);
}

static int compat_iterate_shared(struct file *file, struct dir_context *ctx)
{
	struct filter_ctx fctx = {
		.ctx.actor = filter_actor,
		.ctx.pos = ctx->pos,
		.orig_ctx = ctx,
	};
	int ret;

	if (!orig_root_f_op || !orig_root_f_op->iterate_shared)
		return -ENOTDIR;

	ret = orig_root_f_op->iterate_shared(file, &fctx.ctx);

	/* Sync position back */
	ctx->pos = fctx.ctx.pos;
	return ret;
}

/* ── Module init/exit ───────────────────────────────────────────────── */

static int __init bingux_compat_init(void)
{
	struct path root_path;
	struct inode *root_inode;
	int err;

	memset(&compat_symlink_i_op, 0, sizeof(compat_symlink_i_op));
	compat_symlink_i_op.get_link = compat_get_link;

	err = kern_path("/", LOOKUP_DIRECTORY, &root_path);
	if (err) {
		pr_err("bingux_compat: cannot resolve root path: %d\n", err);
		return err;
	}

	root_inode = root_path.dentry->d_inode;
	if (!root_inode) {
		pr_err("bingux_compat: root inode is NULL\n");
		path_put(&root_path);
		return -ENOENT;
	}

	/* Override inode operations (lookup) */
	orig_root_i_op = root_inode->i_op;
	memcpy(&compat_root_i_op, orig_root_i_op, sizeof(compat_root_i_op));
	compat_root_i_op.lookup = compat_lookup;
	root_inode->i_op = &compat_root_i_op;

	/* Override file operations (readdir/iterate) */
	orig_root_f_op = root_inode->i_fop;
	memcpy(&compat_root_f_op, orig_root_f_op, sizeof(compat_root_f_op));
	compat_root_f_op.iterate_shared = compat_iterate_shared;
	root_inode->i_fop = &compat_root_f_op;

	path_put(&root_path);

	pr_info("bingux_compat: FHS path translation active — "
		"root shows: system  users  io\n");
	return 0;
}

static void __exit bingux_compat_exit(void)
{
	struct path root_path;
	struct inode *root_inode;

	if (kern_path("/", LOOKUP_DIRECTORY, &root_path) == 0) {
		root_inode = root_path.dentry->d_inode;
		if (root_inode) {
			if (root_inode->i_op == &compat_root_i_op)
				root_inode->i_op = orig_root_i_op;
			if (root_inode->i_fop == &compat_root_f_op)
				root_inode->i_fop = orig_root_f_op;
		}
		path_put(&root_path);
	}

	pr_info("bingux_compat: unloaded — FHS translation disabled\n");
}

module_init(bingux_compat_init);
module_exit(bingux_compat_exit);
