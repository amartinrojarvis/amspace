// Disposable app for check-scene-launch.sh. Never activates or opens documents.
#import <AppKit/AppKit.h>
#import <unistd.h>

int main(void) {
    @autoreleasepool {
        NSApplication *app = NSApplication.sharedApplication;
        [app finishLaunching];
        NSNumber *display = [NSBundle.mainBundle objectForInfoDictionaryKey:@"AMSpaceFixtureDisplay"];
        NSScreen *screen = nil;
        for (NSScreen *candidate in NSScreen.screens) {
            if ([candidate.deviceDescription[@"NSScreenNumber"] isEqual:display]) {
                screen = candidate;
                break;
            }
        }
        if (!screen) return 1;
        NSURL *pidFile = [[NSBundle.mainBundle.bundleURL URLByDeletingLastPathComponent]
                            URLByAppendingPathComponent:@"fixture.pid"];
        if (![[NSString stringWithFormat:@"%d", getpid()] writeToURL:pidFile atomically:YES
                    encoding:NSUTF8StringEncoding error:NULL]) return 1;
        NSMutableArray<NSWindow *> *windows = [NSMutableArray array];
        NSRect area = screen.visibleFrame;
        for (int i = 0; i < 3; i++) {
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)i * NSEC_PER_SEC),
                           dispatch_get_main_queue(), ^{
                NSRect content = NSMakeRect(NSMinX(area) + 100 + 30 * i, NSMinY(area) + 100, 360, 250);
                NSWindow *window = [[NSWindow alloc] initWithContentRect:content
                    styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskResizable | NSWindowStyleMaskClosable
                    backing:NSBackingStoreBuffered defer:NO];
                window.releasedWhenClosed = NO;
                window.tabbingMode = NSWindowTabbingModeDisallowed;
                window.level = NSFloatingWindowLevel;
                window.title = [NSString stringWithFormat:@"AMSpace fixture %d", i + 1];
                [windows addObject:window];
                [window orderFrontRegardless];
                [window setFrame:[window frameRectForContentRect:content] display:YES];
            });
        }
        NSURL *hideRequest = [[NSBundle.mainBundle.bundleURL URLByDeletingLastPathComponent]
                               URLByAppendingPathComponent:@"hide"];
        [NSTimer scheduledTimerWithTimeInterval:0.1 repeats:YES block:^(NSTimer *timer) {
            if (![NSFileManager.defaultManager fileExistsAtPath:hideRequest.path]) return;
            [timer invalidate];
            [app hide:nil];
        }];
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 45 * NSEC_PER_SEC),
                       dispatch_get_main_queue(), ^{ [app terminate:nil]; });
        [app run];
    }
    return 0;
}
