//! Icons are embedded on every target so the web build never fetches them from a CDN.

gpui_kit_assets::icon_assets!(pub Assets, [
    // GPUI Kit's default component icons (close buttons, checks, chevrons, …).
    ALargeSmall, ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Asterisk, Ban, Battery,
    BatteryCharging, BatteryFull, BatteryLow, BatteryMedium, BatteryWarning, Bell, BookOpen, Bot,
    Building2, Calendar, CaseSensitive, ChartPie, Check, ChevronDown, ChevronLeft, ChevronRight,
    ChevronUp, ChevronsUpDown, CircleAlert, CircleCheck, CircleUser, CircleX, Close, Copy, Cpu,
    Dash, Delete, Ellipsis, EllipsisVertical, ExternalLink, Eye, EyeOff, File, FileText, Folder,
    FolderClosed, FolderOpen, Frame, GalleryVerticalEnd, Github, Globe, HardDrive, Heart,
    HeartOff, Inbox, Info, Inspector, LayoutDashboard, Loader, LoaderCircle, Map, Maximize,
    MemoryStick, Menu, Mic, Minimize, Minus, Moon, Network, Palette, PanelBottom,
    PanelBottomOpen, PanelLeft, PanelLeftClose, PanelLeftOpen, PanelRight, PanelRightClose,
    PanelRightOpen, Pause, Play, Plus, Redo, Redo2, RefreshCw, Replace, ResizeCorner, RotateCw,
    Search, Settings, Settings2, SortAscending, SortDescending, Square, SquareTerminal, Star,
    StarFill, StarOff, Sun, ThumbsDown, ThumbsUp, TriangleAlert, Undo, Undo2, User, WindowClose,
    WindowMaximize, WindowMinimize, WindowRestore,
    // RGate chrome.
    Activity, AudioWaveform, Box, Circle, CircleDot, ClipboardPaste, Eraser, FileCode, FilePlus,
    Grid3x3, Hand, List, ListTree, Magnet, MousePointer2, Pencil, Plug, RotateCcw, Save, SaveAll,
    Scan, Scissors, ScrollText, SlidersHorizontal, Spline, StepForward, Timer, X, ZoomIn, ZoomOut,
]);
