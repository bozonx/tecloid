/* Standalone GTK/RemoteDesktop probe. Build with libei >= 1.6 and libportal-gtk4. */
#include <errno.h>
#include <gtk/gtk.h>
#include <libei.h>
#include <libportal-gtk4/portal-gtk4.h>
#include <poll.h>

static const char sample[] = "Tecloid 👩🏽‍💻";

typedef struct {
    GtkWindow *window;
    GtkEditable *entry;
    GtkWidget *send;
    GtkLabel *status;
    XdpPortal *portal;
    XdpSession *session;
    struct ei *ei;
    struct ei_device *device;
    uint32_t sequence;
    gint64 sent_at;
    guint timer;
} Probe;

static void report(Probe *probe, const char *message)
{
    gtk_label_set_text(probe->status, message);
    g_print("%s\n", message);
}

static void report_error(Probe *probe, const char *stage, GError *error)
{
    g_autofree char *message = g_strdup_printf("%s: %s", stage, error->message);
    report(probe, message);
    g_error_free(error);
}

/* A seat with text support must be bound before it will offer a text device. */
static const char *read_eis(Probe *probe, int timeout_ms)
{
    struct pollfd fd = { .fd = ei_get_fd(probe->ei), .events = POLLIN };
    gint64 deadline = g_get_monotonic_time() + timeout_ms * 1000;
    do {
        ei_dispatch(probe->ei);
        struct ei_event *event;
        while ((event = ei_get_event(probe->ei))) {
            struct ei_device *device = ei_event_get_device(event);
            enum ei_event_type type = ei_event_get_type(event);
            switch (type) {
            case EI_EVENT_SEAT_ADDED: {
                struct ei_seat *seat = ei_event_get_seat(event);
                if (ei_seat_has_capability(seat, EI_DEVICE_CAP_TEXT))
                    ei_seat_bind_capabilities(seat, EI_DEVICE_CAP_TEXT, NULL);
                else
                    g_print("EIS seat does not advertise EI_DEVICE_CAP_TEXT.\n");
                break;
            }
            case EI_EVENT_DEVICE_RESUMED:
                if (!probe->device && ei_device_has_capability(device, EI_DEVICE_CAP_TEXT))
                    probe->device = ei_device_ref(device);
                break;
            case EI_EVENT_DEVICE_PAUSED:
            case EI_EVENT_DEVICE_REMOVED:
                if (device == probe->device)
                    probe->device = ei_device_unref(probe->device);
                break;
            case EI_EVENT_DISCONNECT:
                ei_event_unref(event);
                return "EIS disconnected.";
            default:
                break;
            }
            ei_event_unref(event);
        }
        if (probe->device || !timeout_ms)
            break;
        gint64 remaining = deadline - g_get_monotonic_time();
        if (remaining <= 0)
            break;
        int ready = poll(&fd, 1, (int)((remaining + 999) / 1000));
        if (ready < 0 && errno != EINTR)
            return "Polling EIS failed.";
    } while (g_get_monotonic_time() < deadline);
    return probe->device ? NULL : "No ready EIS text device (ei_text).";
}

static gboolean verify(gpointer data)
{
    Probe *probe = data;
    probe->timer = 0;
    if (probe->sent_at) {
        probe->sent_at = 0;
        report(probe, "ei_text sent, but the test field did not receive the exact string.");
        gtk_widget_set_sensitive(probe->send, TRUE);
    }
    return G_SOURCE_REMOVE;
}

static void text_changed(GtkEditable *entry, gpointer data)
{
    Probe *probe = data;
    if (!probe->sent_at || !g_str_equal(gtk_editable_get_text(entry), sample))
        return;
    double elapsed = (g_get_monotonic_time() - probe->sent_at) / 1000.0;
    probe->sent_at = 0;
    g_source_remove(probe->timer);
    probe->timer = 0;
    g_autofree char *message = g_strdup_printf(
        "PASS: exact text received via ei_text (send to field: %.2f ms).", elapsed);
    report(probe, message);
    gtk_widget_set_sensitive(probe->send, TRUE);
}

static gboolean send_text(gpointer data)
{
    Probe *probe = data;
    probe->timer = 0;
    GtkWidget *focus = gtk_window_get_focus(probe->window);
    const char *error = read_eis(probe, 0);
    if (error || !gtk_window_is_active(probe->window) || !focus ||
        (focus != GTK_WIDGET(probe->entry) &&
         !gtk_widget_is_ancestor(focus, GTK_WIDGET(probe->entry)))) {
        report(probe, error ? error : "Focus left the test field. No text sent.");
        gtk_widget_set_sensitive(probe->send, TRUE);
        return G_SOURCE_REMOVE;
    }

    report(probe, "ei_text sent. Waiting for the test field…");
    probe->timer = g_timeout_add(1500, verify, probe);
    probe->sent_at = g_get_monotonic_time();
    ei_device_start_emulating(probe->device, ++probe->sequence);
    ei_device_text_utf8(probe->device, sample);
    ei_device_frame(probe->device, ei_now(probe->ei));
    ei_device_stop_emulating(probe->device);
    return G_SOURCE_REMOVE;
}

static void send_clicked(GtkButton *button, gpointer data)
{
    Probe *probe = data;
    gtk_widget_set_sensitive(GTK_WIDGET(button), FALSE);
    gtk_editable_set_text(probe->entry, "");
    gtk_widget_grab_focus(GTK_WIDGET(probe->entry));
    /* Let GTK announce the new text-input focus before sending the sample. */
    probe->timer = g_timeout_add(200, send_text, probe);
}

static void session_started(GObject *source, GAsyncResult *result, gpointer data)
{
    Probe *probe = data;
    GError *error = NULL;
    if (!xdp_session_start_finish(XDP_SESSION(source), result, &error)) {
        report_error(probe, "RemoteDesktop Start", error);
        return;
    }
    int fd = xdp_session_connect_to_eis(probe->session, &error);
    if (fd < 0) {
        report_error(probe, "RemoteDesktop ConnectToEIS", error);
        return;
    }
    probe->ei = ei_new_sender(NULL);
    ei_configure_name(probe->ei, "Tecloid ei_text probe");
    if (ei_setup_backend_fd(probe->ei, fd) < 0) {
        report(probe, "Could not set up EIS.");
        return;
    }
    const char *failure = read_eis(probe, 2000);
    report(probe, failure ? failure : "EIS text device ready. Click Insert sample.");
    gtk_widget_set_sensitive(probe->send, failure == NULL);
}

static void session_created(GObject *source, GAsyncResult *result, gpointer data)
{
    Probe *probe = data;
    GError *error = NULL;
    probe->session = xdp_portal_create_remote_desktop_session_finish(
        XDP_PORTAL(source), result, &error);
    if (!probe->session) {
        report_error(probe, "RemoteDesktop CreateSession/SelectDevices", error);
        return;
    }
    XdpParent *parent = xdp_parent_new_gtk(probe->window);
    xdp_session_start(probe->session, parent, NULL, session_started, probe);
    xdp_parent_free(parent);
}

static void connect_clicked(GtkButton *button, gpointer data)
{
    Probe *probe = data;
    gtk_widget_set_sensitive(GTK_WIDGET(button), FALSE);
    report(probe, "Waiting for RemoteDesktop permission…");
    xdp_portal_create_remote_desktop_session_full(
        probe->portal, XDP_DEVICE_KEYBOARD, XDP_OUTPUT_NONE,
        XDP_REMOTE_DESKTOP_FLAG_NONE, XDP_CURSOR_MODE_HIDDEN,
        XDP_PERSIST_MODE_NONE, NULL, NULL, session_created, probe);
}

static void activate(GtkApplication *app, gpointer data)
{
    Probe *probe = data;
    probe->window = GTK_WINDOW(gtk_application_window_new(app));
    gtk_window_set_title(probe->window, "Tecloid ei_text probe");
    gtk_window_set_default_size(probe->window, 520, 175);

    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 10);
    gtk_widget_set_margin_start(box, 16);
    gtk_widget_set_margin_end(box, 16);
    gtk_widget_set_margin_top(box, 16);
    gtk_widget_set_margin_bottom(box, 16);
    gtk_window_set_child(probe->window, box);

    GtkWidget *connect = gtk_button_new_with_label("Connect via RemoteDesktop");
    probe->send = gtk_button_new_with_label("Insert sample");
    probe->entry = GTK_EDITABLE(gtk_entry_new());
    probe->status = GTK_LABEL(gtk_label_new("Connect to check for ei_text."));
    gtk_label_set_wrap(probe->status, TRUE);
    gtk_widget_set_sensitive(probe->send, FALSE);

    gtk_box_append(GTK_BOX(box), connect);
    gtk_box_append(GTK_BOX(box), GTK_WIDGET(probe->entry));
    gtk_box_append(GTK_BOX(box), probe->send);
    gtk_box_append(GTK_BOX(box), GTK_WIDGET(probe->status));

    g_signal_connect(connect, "clicked", G_CALLBACK(connect_clicked), probe);
    g_signal_connect(probe->send, "clicked", G_CALLBACK(send_clicked), probe);
    g_signal_connect(probe->entry, "changed", G_CALLBACK(text_changed), probe);
    gtk_window_present(probe->window);
}

int main(int argc, char **argv)
{
    Probe probe = { .portal = xdp_portal_new() };
    GtkApplication *app = gtk_application_new(
        "dev.bozonx.tecloid.EiTextProbe", G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(activate), &probe);
    int result = g_application_run(G_APPLICATION(app), argc, argv);

    if (probe.timer)
        g_source_remove(probe.timer);
    if (probe.device)
        ei_device_unref(probe.device);
    if (probe.ei)
        ei_unref(probe.ei);
    if (probe.session) {
        xdp_session_close(probe.session);
        g_object_unref(probe.session);
    }
    g_object_unref(probe.portal);
    g_object_unref(app);
    return result;
}
