require 'shiika/program'
require 'shiika/evaluator'
require 'shiika/type'

module Shiika
  module Stdlib
    include Shiika::Type
    SkObj = Shiika::Evaluator::SkObj

    CLASSES = [
      {
        name: "Object",
        parent: :noparent,
        initializer: {
          params: [],
          body: ->(){}
        },
        ivars: [],
        methods: []
      },
      {
        name: "Int",
        parent: "Object",
        initializer: {
          params: [],
          body: ->(){}
        },
        ivars: {},
        methods: [
          {
            name: "+",
            ret_type_name: "Int",
            param_type_names: ["Int"],
            body: ->(this, other){
              n = this.ivar_values[0] + other.ivar_values[0]
              SkObj.new('Int', [n])
            }
          }
        ]
      }
    ]

    # Build Program::XX from CLASSES
    def self.sk_classes
      CLASSES.flat_map{|spec|
        init = Program::SkInitializer.new(
          spec[:name], spec[:initializer][:params], spec[:initializer][:body]
        )
        sk_methods = spec[:methods].map{|x|
          params = x[:param_type_names].map{|ty_name|
            Program::Param.new("(no name)", ty_name)
          }
          sk_method = Program::SkMethod.new(
            x[:name], params, x[:ret_type_name], x[:body]
          )
          [x[:name], sk_method]
        }.to_h
        sk_class, meta_class = Program::SkClass.build(
          spec[:name], spec[:parent], init,
          spec[:ivars], {}, sk_methods
        )
        [[sk_class.name, sk_class],
         [meta_class.name, meta_class]]
      }.to_h
    end
  end
end
