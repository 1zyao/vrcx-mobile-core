/**
 * Port checklist for the mobile UI.
 *
 * The layout, information architecture and page set come from VRCM 1.1.4
 * (`D:\vrcm\_upstream-1.1.4\composeApp\src\commonMain\kotlin\presentation`).
 * `source` points at the Kotlin file the page is being ported from, so each
 * route can be checked off against its original.
 *
 * Only the data layer is ours: every page reads through VRCX-0 repositories and
 * stores, never through a database connection of its own.
 */
export type MobileScreenStatus = 'ported' | 'in-progress' | 'stub';

export type MobileScreen = {
    /** Route below `/m`. Use `''` for the shell index. */
    path: string;
    /** i18n key for the title, when the project already has one. */
    titleKey?: string;
    /** VRCM file this page is ported from. */
    source: string;
    status: MobileScreenStatus;
};

export const MOBILE_SCREENS: MobileScreen[] = [
    {
        path: 'home',
        source: 'screens/home/HomeScreen.kt',
        status: 'in-progress'
    },
    {
        path: 'favorites',
        source: 'screens/favorites/FavoritesScreen.kt',
        status: 'stub'
    },
    {
        path: 'inbox',
        source: 'screens/notification/NotificationScreen.kt',
        status: 'stub'
    },
    {
        path: 'search',
        source: 'screens/search/SearchScreen.kt',
        status: 'stub'
    },
    {
        path: 'activity',
        source: 'screens/activity/ActivityScreen.kt',
        status: 'stub'
    },
    {
        path: 'user/:userId',
        source: 'screens/user/UserScreen.kt',
        status: 'stub'
    },
    {
        path: 'world/:worldId',
        source: 'screens/world/WorldScreen.kt',
        status: 'stub'
    },
    {
        path: 'avatar/:avatarId',
        source: 'screens/avatar/AvatarScreen.kt',
        status: 'stub'
    },
    {
        path: 'group/:groupId',
        source: 'screens/group/GroupScreen.kt',
        status: 'stub'
    },
    {
        path: 'gallery',
        source: 'screens/gallery/GalleryScreen.kt',
        status: 'stub'
    },
    {
        path: 'inventory',
        source: 'screens/inventory/InventoryScreen.kt',
        status: 'stub'
    },
    {
        path: 'meetup',
        source: 'screens/meetup/MeetupScreen.kt',
        status: 'stub'
    },
    {
        path: 'settings',
        source: 'screens/settings/SettingsScreen.kt',
        status: 'stub'
    },
    {
        path: 'auth',
        source: 'screens/auth/AuthScreen.kt',
        status: 'stub'
    }
];

/** Bottom navigation, matching VRCM's Home / Favorites / Inbox + search button. */
export const MOBILE_NAV_ITEMS = ['home', 'favorites', 'inbox'] as const;
