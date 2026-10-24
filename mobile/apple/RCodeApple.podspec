Pod::Spec.new do |s|
  s.name = 'RCodeApple'
  s.version = '0.0.1'
  s.summary = 'Native iOS wrapper for the RCode decoder.'
  s.homepage = 'https://github.com/rcode/rcode'
  s.license = { :type => 'MIT' }
  s.author = { 'RCode' => 'maintainers@example.com' }
  s.source = { :path => '.' }
  s.source_files = 'Sources/RCodeApple/**/*.swift'
  s.vendored_frameworks = 'RCode.xcframework'
  s.platform = :ios, '15.0'
  s.swift_version = '5.9'
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES', 'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'x86_64' }
end
