Pod::Spec.new do |s|
  s.name = 'RCodeApple'
  s.version = '0.0.1'
  s.summary = 'Native iOS wrapper for the RCode decoder.'
  s.homepage = 'https://github.com/rcode/rcode'
  s.license = { :type => 'MIT' }
  s.author = { 'RCode' => 'maintainers@example.com' }
  s.source = { :path => '.' }
  s.source_files = 'Sources/RCodeApple/**/*.swift'
  s.platform = :ios, '15.0'
  s.swift_version = '5.9'
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'HEADER_SEARCH_PATHS' => '$(inherited) "$(PODS_TARGET_SRCROOT)/RCode.xcframework/ios-arm64-simulator/Headers"',
    'OTHER_SWIFT_FLAGS' => '$(inherited) -Xcc -fmodule-map-file=$(PODS_TARGET_SRCROOT)/RCode.xcframework/ios-arm64-simulator/Headers/module.modulemap',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'x86_64',
    'LIBRARY_SEARCH_PATHS[sdk=iphonesimulator*]' => '$(inherited) "$(PODS_TARGET_SRCROOT)/RCode.xcframework/ios-arm64-simulator"',
    'LIBRARY_SEARCH_PATHS[sdk=iphoneos*]' => '$(inherited) "$(PODS_TARGET_SRCROOT)/RCode.xcframework/ios-arm64"',
    'OTHER_LDFLAGS[sdk=iphonesimulator*]' => '$(inherited) -lrcode',
    'OTHER_LDFLAGS[sdk=iphoneos*]' => '$(inherited) -lrcode',
  }
end
