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

GtkWidget* dfo_webview_new(const char* url, const char* probe_js)
{
    WebKitSettings* settings = webkit_settings_new();

    WebKitFeatureList* features = webkit_settings_get_all_features();
    WebKitFeature* feature = webkit_feature_list_find(features, "PreferPageRenderingUpdatesNear60FPS");
    if (feature)
        webkit_settings_set_feature_enabled(settings, feature, FALSE);
    webkit_feature_list_unref(features);

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

    gtk_widget_set_hexpand(GTK_WIDGET(view), TRUE);
    gtk_widget_set_vexpand(GTK_WIDGET(view), TRUE);
    webkit_web_view_load_uri(view, url);
    return GTK_WIDGET(view);
}
