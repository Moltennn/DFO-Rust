/*
 * The native shell's WebKitGTK 6.0 shim.
 *
 * There is no usable Rust binding for the 6.0 API (the `webkitgtk` crate is an
 * unpublished placeholder), so this exposes the few calls the shell needs:
 * create a WebView with the near-60 feature already off, inject the page probe
 * at document start, and load the URL.
 *
 * 6.0 dropped webkit_web_view_new_with_settings(); the settings are applied to
 * a fresh view instead, still before the first load, so the web process starts
 * with the feature off.
 */
#include <gtk/gtk.h>
#include <webkit/webkit.h>

/*
 * The game asks for pointer lock on a click (WebCore's pointer lock path).
 * WebKitGTK routes that through WebKitWebView::permission-request, and the
 * request stays pending until the app decides, so without this handler the
 * pointer is never locked: the cursor stays visible and the game falls back to
 * its slow, unlocked look.
 */
static gboolean dfo_permission_request(WebKitWebView* view, WebKitPermissionRequest* request, gpointer user_data)
{
    (void)view;
    (void)user_data;
    if (WEBKIT_IS_POINTER_LOCK_PERMISSION_REQUEST(request)) {
        webkit_permission_request_allow(request);
        return TRUE;
    }
    return FALSE;
}

/*
 * The probe's channel (window.webkit.messageHandlers.dfo): its snapshots,
 * fps/error reports and F12 land here. There is no Rust IPC in this shell,
 * so the shim prints them and opens the inspector.
 */
static void dfo_probe_message(WebKitUserContentManager* manager, JSCValue* value, gpointer user_data)
{
    (void)manager;
    if (!jsc_value_is_string(value))
        return;
    char* text = jsc_value_to_string(value);
    if (!text)
        return;
    if (g_strstr_len(text, -1, "\"kind\":\"devtools\"")) {
        WebKitWebInspector* inspector = webkit_web_view_get_inspector(WEBKIT_WEB_VIEW(user_data));
        webkit_web_inspector_show(inspector);
    } else {
        g_printerr("[probe] %s\n", text);
    }
    g_free(text);
}

GtkWidget* dfo_webview_new(const char* url, const char* probe_js)
{
    WebKitSettings* settings = webkit_settings_new();

    WebKitFeatureList* features = webkit_settings_get_all_features();
    WebKitFeature* feature = webkit_feature_list_find(features, "PreferPageRenderingUpdatesNear60FPS");
    if (feature)
        webkit_settings_set_feature_enabled(settings, feature, FALSE);
    webkit_feature_list_unref(features);
    webkit_settings_set_enable_developer_extras(settings, TRUE);

    WebKitWebView* view = WEBKIT_WEB_VIEW(webkit_web_view_new());
    webkit_web_view_set_settings(view, settings);
    g_object_unref(settings);

    WebKitUserContentManager* manager = webkit_web_view_get_user_content_manager(view);
    WebKitUserScript* script = webkit_user_script_new(probe_js,
        WEBKIT_USER_CONTENT_INJECT_TOP_FRAME,
        WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START,
        NULL, NULL);
    webkit_user_content_manager_add_script(manager, script);
    webkit_user_script_unref(script);

    g_signal_connect(manager, "script-message-received::dfo", G_CALLBACK(dfo_probe_message), view);
    webkit_user_content_manager_register_script_message_handler(manager, "dfo", NULL);

    g_signal_connect(view, "permission-request", G_CALLBACK(dfo_permission_request), NULL);

    gtk_widget_set_hexpand(GTK_WIDGET(view), TRUE);
    gtk_widget_set_vexpand(GTK_WIDGET(view), TRUE);
    webkit_web_view_load_uri(view, url);
    return GTK_WIDGET(view);
}
