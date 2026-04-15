/*
 * bingux_compat — Kernel module for transparent FHS path translation
 *
 * Bingux uses a three-directory root: /system, /users, /io
 * No other entries exist at /.  FHS compatibility is achieved by
 * overriding the root inode's lookup operation so that path resolution
 * for /bin, /usr, /etc, etc. transparently follows into the Bingux
 * hierarchy.
 *
 *   ls /           → system  users  io    (only real dentries)
 *   ls /etc        → works   (resolves /system/state/ephemeral/etc)
 *   cat /bin/sh    → works   (resolves /system/profiles/current/bin/sh)
 *   readlink /bin  → /system/profiles/current/bin  (virtual symlink)
 *
 * Implementation:
 *   On load, we replace the root inode's i_op->lookup with our own.
 *   When the VFS resolves an FHS name at /, our lookup creates a
 *   virtual symlink inode (on the root superblock) whose get_link
 *   returns the Bingux target path.  The VFS follows the symlink
 *   transparently, crossing mount boundaries correctly.
 *
 *   Since no real dentries exist for FHS names, readdir (ls /)
 *   naturally shows only system, users, io.  The virtual symlinks
 *   are instantiated on demand during path resolution and are
 *   managed by the dcache like any other negative/positive dentry.
 *
 *   On unload, we restore the original i_op.
 */

#include <linux/module.h>
#include <linux/kernel.h>
#include <linux/fs.h>
#include <linux/dcache.h>
#include <linux/namei.h>
#include <linux/string.h>
#include <linux/slab.h>
#include <linux/version.h>
#include <linux/stringhash.h>

MODULE_LICENSE("GPL");
MODULE_AUTHOR("Bingux Project");
MODULE_DESCRIPTION("Transparent FHS path translation for Bingux root layout");
MODULE_VERSION("1.0");

/*
 * FHS-to-Bingux path mapping table.
 *
 * When a process resolves /bin/sh, the VFS calls lookup("bin") on
 * the root inode.  We intercept that and return a virtual symlink
 * inode pointing to the Bingux path.
 */
struct compat_mapping {
	const char *fhs_name;     /* Name looked up at / (e.g. "bin") */
	const char *bingux_path;  /* Absolute Bingux path to redirect to */
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

	/*
	 * /dev is NOT handled here — it needs a real bind mount
	 * from /io so that realpath("/dev/...") stays as "/dev/..."
	 * rather than resolving through a symlink to "/io/...".
	 * Programs like seatd validate device paths against /dev/.
	 * Init handles this: mkdir /dev && mount --bind /io /dev
	 */

	/*
	 * /proc and /sys are also handled via bind mounts in init,
	 * like /dev, so that realpath() returns /proc/... and /sys/...
	 * as programs expect.  Init does:
	 *   mkdir /proc && mount --bind /system/kernel/proc /proc
	 *   mkdir /sys  && mount --bind /system/kernel/sys  /sys
	 */

	/* Boot */
	{ "boot",   "/system/boot"                   },

	/* Misc FHS dirs */
	{ "mnt",    "/system/state/ephemeral/mnt"    },
	{ "media",  "/system/state/ephemeral/media"  },
	{ "srv",    "/system/state/ephemeral/srv"    },

	{ NULL, NULL }
};

/* Saved original inode operations so we can restore on unload */
static const struct inode_operations *orig_root_i_op;
static struct inode_operations compat_root_i_op;

/* Inode operations for virtual symlink inodes */
static struct inode_operations compat_symlink_i_op;

/*
 * Find the mapping for an FHS name.
 */
static const struct compat_mapping *find_mapping(const char *name)
{
	const struct compat_mapping *m;

	for (m = mappings; m->fhs_name; m++) {
		if (strcmp(name, m->fhs_name) == 0)
			return m;
	}
	return NULL;
}

/*
 * get_link for our virtual symlinks.
 *
 * The target path string is stored in inode->i_link (set when we
 * create the inode).  The VFS follows this like a regular symlink,
 * correctly crossing mount boundaries.
 *
 * We use DELAYED_CALL with no destructor since the link target is
 * a static string from our mapping table (module lifetime).
 */
static const char *compat_get_link(struct dentry *dentry, struct inode *inode,
				   struct delayed_call *done)
{
	if (!inode)
		return ERR_PTR(-ECHILD);
	/* i_link was set to the static mapping string on creation */
	return inode->i_link;
}

/*
 * Create a virtual symlink inode on the root superblock.
 *
 * This inode:
 *   - Lives on the root superblock (no cross-mount confusion)
 *   - Has S_IFLNK type so the VFS follows it during path resolution
 *   - Has get_link returning the Bingux target path
 *   - Is ephemeral — the dcache manages its lifetime
 *   - Never hits disk (rootfs is tmpfs/ramfs in initramfs,
 *     and even on ext4 we never create a real dirent)
 */
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
	inode->i_link = (char *)target;  /* static string, no free needed */
	inode->i_size = strlen(target);

	return inode;
}

/*
 * Our replacement lookup for the root inode.
 *
 * For FHS compat names: create a virtual symlink inode and splice
 * it into the dentry.  The VFS will follow it transparently.
 *
 * For real names (system, users, io) or unknown names: delegate
 * to the original filesystem's lookup.
 */
static struct dentry *compat_lookup(struct inode *dir, struct dentry *dentry,
				    unsigned int flags)
{
	const struct compat_mapping *m;
	struct inode *inode;

	m = find_mapping(dentry->d_name.name);
	if (!m) {
		/* Not an FHS name — delegate to original lookup */
		if (orig_root_i_op && orig_root_i_op->lookup)
			return orig_root_i_op->lookup(dir, dentry, flags);
		d_add(dentry, NULL);
		return NULL;
	}

	/*
	 * Create a virtual symlink for this FHS name.
	 * The VFS will follow it to the real Bingux path.
	 */
	inode = make_virtual_symlink(dir->i_sb, m->bingux_path);
	if (!inode) {
		d_add(dentry, NULL);
		return NULL;
	}

	d_add(dentry, inode);
	return NULL;
}

/*
 * Invalidate any cached dentries for FHS names in the root directory.
 *
 * During early boot (before this module loads), the kernel may create
 * real directories like /dev on the rootfs and cache their dentries.
 * We need to evict those so that subsequent lookups hit our
 * compat_lookup and get redirected.
 */
static void invalidate_fhs_dentries(struct dentry *root)
{
	const struct compat_mapping *m;
	struct dentry *child;

	for (m = mappings; m->fhs_name; m++) {
		struct qstr name = QSTR_INIT(m->fhs_name,
					     strlen(m->fhs_name));
		child = d_hash_and_lookup(root, &name);
		if (!child)
			continue;

		pr_debug("bingux_compat: evicting cached dentry /%s\n",
			 m->fhs_name);

		/*
		 * d_invalidate unhashes the dentry and evicts it from the
		 * dcache, along with its entire subtree.  Next time userspace
		 * accesses this name, the VFS will call our compat_lookup.
		 */
		d_invalidate(child);
		dput(child);
	}
}

/*
 * Module init: hook the root inode's lookup.
 */
static int __init bingux_compat_init(void)
{
	struct path root_path;
	struct inode *root_inode;
	int err;

	/* Set up our symlink inode operations */
	memset(&compat_symlink_i_op, 0, sizeof(compat_symlink_i_op));
	compat_symlink_i_op.get_link = compat_get_link;

	/* Get the root dentry */
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

	/* Save original operations */
	orig_root_i_op = root_inode->i_op;

	/* Build our replacement: copy all ops, override lookup */
	memcpy(&compat_root_i_op, orig_root_i_op, sizeof(compat_root_i_op));
	compat_root_i_op.lookup = compat_lookup;

	/*
	 * Swap in our operations.  Single pointer write — effectively
	 * atomic w.r.t. concurrent lookups.
	 */
	root_inode->i_op = &compat_root_i_op;

	/*
	 * NOTE: We do NOT call d_invalidate on ramfs/tmpfs dentries.
	 * On ramfs the dentries ARE the data — invalidating them would
	 * destroy real directories.  Pre-existing entries like the
	 * kernel's early-boot /dev are handled by bind mounts in init
	 * (which override them before this module loads).
	 *
	 * For disk-backed rootfs (ext4, etc.), you may need to add
	 * dentry invalidation here.  For initramfs, it's not needed
	 * and would be destructive.
	 */

	path_put(&root_path);

	pr_info("bingux_compat: FHS path translation active — "
		"root shows: system  users  io\n");
	return 0;
}

/*
 * Module exit: restore original root inode operations.
 */
static void __exit bingux_compat_exit(void)
{
	struct path root_path;
	struct inode *root_inode;

	if (kern_path("/", LOOKUP_DIRECTORY, &root_path) == 0) {
		root_inode = root_path.dentry->d_inode;
		if (root_inode && root_inode->i_op == &compat_root_i_op)
			root_inode->i_op = orig_root_i_op;
		path_put(&root_path);
	}

	pr_info("bingux_compat: unloaded — FHS translation disabled\n");
}

module_init(bingux_compat_init);
module_exit(bingux_compat_exit);
