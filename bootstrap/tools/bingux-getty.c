/*
 * bingux-getty — minimal login prompt for Bingux Linux.
 *
 * Usage: bingux-getty [tty]
 *   tty defaults to /dev/console when omitted.
 *
 * Workflow:
 *   1. Open the TTY and attach stdin/stdout/stderr.
 *   2. Display "bingux login: " prompt.
 *   3. Read a username.
 *   4. Look the user up in /etc/passwd.
 *   5. Set uid, gid, HOME, USER, SHELL, PATH.
 *   6. Exec the user's shell.
 *
 * Compile: bingux-gcc -static -o bingux-getty bingux-getty.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <grp.h>
#include <pwd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/types.h>
#include <unistd.h>

#define MAX_USERNAME 64
#define DEFAULT_PATH "/usr/local/bin:/usr/bin:/bin:/system/profiles/current/bin"

static void die(const char *msg)
{
    perror(msg);
    _exit(1);
}

/* Open the tty and make it our controlling terminal + stdin/stdout/stderr. */
static void open_tty(const char *tty_path)
{
    int fd;

    /* Create a new session so we can acquire a controlling terminal. */
    setsid();

    fd = open(tty_path, O_RDWR | O_NOCTTY);
    if (fd < 0)
        die(tty_path);

    /* Make this the controlling terminal. */
    if (ioctl(fd, TIOCSCTTY, 0) < 0)
        die("TIOCSCTTY");

    /* Wire up standard file descriptors. */
    dup2(fd, STDIN_FILENO);
    dup2(fd, STDOUT_FILENO);
    dup2(fd, STDERR_FILENO);

    if (fd > STDERR_FILENO)
        close(fd);
}

/* Strip trailing newline / carriage-return characters. */
static void chomp(char *s)
{
    size_t len = strlen(s);
    while (len > 0 && (s[len - 1] == '\n' || s[len - 1] == '\r'))
        s[--len] = '\0';
}

int main(int argc, char *argv[])
{
    const char *tty_path;
    char username[MAX_USERNAME];
    struct passwd *pw;

    /* Determine which tty to use. */
    if (argc >= 2) {
        /* If the argument is a bare name like "tty1", prefix /dev/. */
        if (argv[1][0] == '/')
            tty_path = argv[1];
        else {
            static char buf[256];
            snprintf(buf, sizeof(buf), "/dev/%s", argv[1]);
            tty_path = buf;
        }
    } else {
        tty_path = "/dev/console";
    }

    open_tty(tty_path);

    for (;;) {
        /* Clear the environment for a clean login. */
        printf("\nbingux login: ");
        fflush(stdout);

        if (fgets(username, sizeof(username), stdin) == NULL) {
            /* EOF — pause briefly and retry (e.g. after vhangup). */
            sleep(1);
            clearerr(stdin);
            continue;
        }

        chomp(username);

        if (username[0] == '\0')
            continue;

        /* Look up user in /etc/passwd. */
        pw = getpwnam(username);
        if (pw == NULL) {
            printf("Login incorrect\n");
            sleep(2);
            continue;
        }

        /* Set supplementary groups. */
        if (initgroups(pw->pw_name, pw->pw_gid) < 0)
            die("initgroups");

        /* Set gid then uid (must be in this order). */
        if (setgid(pw->pw_gid) < 0)
            die("setgid");
        if (setuid(pw->pw_uid) < 0)
            die("setuid");

        /* Build environment. */
        clearenv();
        setenv("HOME", pw->pw_dir, 1);
        setenv("USER", pw->pw_name, 1);
        setenv("LOGNAME", pw->pw_name, 1);
        setenv("SHELL", pw->pw_shell, 1);
        setenv("PATH", DEFAULT_PATH, 1);
        setenv("TERM", "linux", 1);

        /* Change to home directory. */
        if (chdir(pw->pw_dir) < 0) {
            /* Fall back to / if home doesn't exist yet. */
            chdir("/");
        }

        /* Exec the user's shell as a login shell (prefix with '-'). */
        {
            const char *shell = pw->pw_shell;
            const char *shell_basename;
            char login_name[256];

            if (shell == NULL || shell[0] == '\0')
                shell = "/bin/sh";

            shell_basename = strrchr(shell, '/');
            if (shell_basename)
                shell_basename++;
            else
                shell_basename = shell;

            snprintf(login_name, sizeof(login_name), "-%s", shell_basename);
            execlp(shell, login_name, (char *)NULL);
            die("exec");
        }
    }

    return 0;
}
